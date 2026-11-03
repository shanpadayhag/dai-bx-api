use sqlx::PgPool;
use uuid::Uuid;

use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card::CardChanges;
use crate::features::cards::error::CardError;

pub struct UpdateCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    build_board: BuildBoard,
    postgres_card_source: PostgresCardSource,
}

impl<'a> UpdateCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            build_board: BuildBoard::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        card_changes: &CardChanges,
    ) -> Result<Board, CardError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut update_transaction, user_id, card_id)
            .await?;
        let current_due = self
            .postgres_card_source
            .find_due(&mut *update_transaction, board_summary.id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        let changed_due = card_changes.apply_to_due(current_due)?;
        self.postgres_card_source
            .update(
                &mut *update_transaction,
                board_summary.id,
                card_id,
                card_changes,
                &changed_due,
            )
            .await?;
        let board = self
            .build_board
            .execute(&mut update_transaction, board_summary)
            .await?;
        update_transaction.commit().await?;
        Ok(board)
    }
}
