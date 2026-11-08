use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::card_schedules::actions::end_blocks_of_deleted_cards::EndBlocksOfDeletedCards;
use crate::features::card_schedules::entities::card_schedule::DeletedCards;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::error::CardError;

pub struct DeleteCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    close_running_events: CloseRunningEvents,
    end_blocks_of_deleted_cards: EndBlocksOfDeletedCards,
    build_board: BuildBoard,
    postgres_card_source: PostgresCardSource,
}

impl<'a> DeleteCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            close_running_events: CloseRunningEvents::new(),
            end_blocks_of_deleted_cards: EndBlocksOfDeletedCards::new(),
            build_board: BuildBoard::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        now: UtcInstant,
    ) -> Result<Board, CardError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut delete_transaction, user_id, card_id)
            .await?;
        self.close_running_events
            .execute(
                &mut delete_transaction,
                user_id,
                ClosingScope::Card(card_id),
            )
            .await?;
        self.end_blocks_of_deleted_cards
            .execute(
                &mut delete_transaction,
                user_id,
                DeletedCards::Card(card_id),
                now,
            )
            .await?;
        let removed_place = self
            .postgres_card_source
            .delete(&mut *delete_transaction, board_summary.id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        self.postgres_card_source
            .close_position_gap(&mut *delete_transaction, removed_place)
            .await?;
        let board = self
            .build_board
            .execute(&mut delete_transaction, board_summary)
            .await?;
        delete_transaction.commit().await?;
        Ok(board)
    }
}
