use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::card_schedules::actions::resume_schedules_of_restored_card::ResumeSchedulesOfRestoredCard;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card_move::{CardMoveReason, MoveCause, NewCardMove};
use crate::features::cards::error::CardError;

pub struct RestoreCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    resume_schedules_of_restored_card: ResumeSchedulesOfRestoredCard,
    build_board: BuildBoard,
    postgres_card_source: PostgresCardSource,
    postgres_card_move_source: PostgresCardMoveSource,
}

impl<'a> RestoreCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            resume_schedules_of_restored_card: ResumeSchedulesOfRestoredCard::new(),
            build_board: BuildBoard::new(),
            postgres_card_source: PostgresCardSource,
            postgres_card_move_source: PostgresCardMoveSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, CardError> {
        let mut restore_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut restore_transaction, user_id, card_id)
            .await?;
        let archived_from_list_id = self
            .postgres_card_source
            .find_archived_from(&mut *restore_transaction, board_summary.id, card_id)
            .await?
            .ok_or_else(|| CardError::Conflict(String::from("This card isn’t archived.")))?;
        let restored_list_id = match archived_from_list_id {
            Some(archived_from_list_id) => archived_from_list_id,
            None => self
                .postgres_card_source
                .find_first_list_id(&mut *restore_transaction, board_summary.id)
                .await?
                .ok_or_else(|| CardError::Conflict(String::from("Add a column first.")))?,
        };
        self.postgres_card_source
            .put_back_on_top(&mut *restore_transaction, card_id, restored_list_id)
            .await?;
        self.postgres_card_move_source
            .record(
                &mut *restore_transaction,
                &NewCardMove {
                    card_id,
                    from_list_id: None,
                    to_list_id: Some(restored_list_id),
                    cause: &MoveCause::without_task(CardMoveReason::Restored, now),
                },
            )
            .await?;
        self.resume_schedules_of_restored_card
            .execute(&mut restore_transaction, user_id, card_id, now)
            .await?;
        let board = self
            .build_board
            .execute(
                &mut restore_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        restore_transaction.commit().await?;
        Ok(board)
    }
}
