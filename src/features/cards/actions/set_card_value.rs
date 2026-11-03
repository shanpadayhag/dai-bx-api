use sqlx::PgPool;
use uuid::Uuid;

use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::{Board, CardValueContent};
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_value_source::PostgresCardValueSource;
use crate::features::cards::entities::card::{
    ValueSave, ensure_value_fits_kind, unknown_option_error,
};
use crate::features::cards::error::CardError;

pub struct SetCardValue<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    build_board: BuildBoard,
    postgres_card_value_source: PostgresCardValueSource,
}

impl<'a> SetCardValue<'a> {
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
        card_value_content: &CardValueContent,
    ) -> Result<Board, CardError> {
        let mut set_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut set_transaction, user_id, card_id)
            .await?;
        let property_kind = self
            .postgres_card_value_source
            .find_property_kind(&mut *set_transaction, board_summary.id, property_id)
            .await?
            .ok_or(CardError::NotFound)?;
        ensure_value_fits_kind(card_value_content, property_kind)?;
        let value_save = self
            .postgres_card_value_source
            .upsert(
                &mut *set_transaction,
                user_id,
                board_summary.id,
                card_id,
                property_id,
                card_value_content,
            )
            .await?;
        if value_save == ValueSave::UnknownOption {
            return Err(unknown_option_error());
        }
        let board = self
            .build_board
            .execute(&mut set_transaction, board_summary)
            .await?;
        set_transaction.commit().await?;
        Ok(board)
    }
}
