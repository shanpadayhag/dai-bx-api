use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_value_source::PostgresCardValueSource;
use crate::features::cards::error::CardError;

pub struct ClearCardValue<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    build_board: BuildBoard,
    postgres_card_value_source: PostgresCardValueSource,
}

impl<'a> ClearCardValue<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            build_board: BuildBoard::new(),
            postgres_card_value_source: PostgresCardValueSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        property_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, CardError> {
        let mut clear_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut clear_transaction, user_id, card_id)
            .await?;
        self.postgres_card_value_source
            .find_property_kind(&mut *clear_transaction, board_summary.id, property_id)
            .await?
            .ok_or(CardError::NotFound)?;
        self.postgres_card_value_source
            .delete(&mut *clear_transaction, card_id, property_id)
            .await?;
        let board = self
            .build_board
            .execute(
                &mut clear_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        clear_transaction.commit().await?;
        Ok(board)
    }
}
