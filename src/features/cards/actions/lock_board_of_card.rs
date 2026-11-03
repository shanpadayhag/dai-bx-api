use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::BoardSummary;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct LockBoardOfCard {
    lock_board: LockBoard,
    postgres_card_source: PostgresCardSource,
}

impl LockBoardOfCard {
    pub fn new() -> Self {
        Self {
            lock_board: LockBoard::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<BoardSummary, CardError> {
        let board_id = self
            .postgres_card_source
            .find_board_id(&mut *connection, user_id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        Ok(self
            .lock_board
            .execute(&mut *connection, user_id, board_id)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };

    #[sqlx::test]
    async fn locks_the_board_of_only_the_owner_s_card(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog"]).await;
        let card_id = seed_card(
            &db_pool,
            owner_id,
            board.board.id,
            board.lists[0].id,
            "Plan",
        )
        .await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockBoardOfCard::new()
            .execute(&mut lock_transaction, owner_id, card_id)
            .await
            .expect("the owner should lock the card's board");
        let foreign_lock = LockBoardOfCard::new()
            .execute(&mut lock_transaction, other_person_id, card_id)
            .await;

        assert_eq!(owner_lock, board.board);
        assert!(matches!(foreign_lock, Err(CardError::NotFound)));
    }
}
