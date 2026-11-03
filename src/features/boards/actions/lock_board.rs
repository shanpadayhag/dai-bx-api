use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::BoardSummary;
use crate::features::boards::error::BoardError;

#[derive(Default)]
pub struct LockBoard {
    postgres_board_source: PostgresBoardSource,
}

impl LockBoard {
    pub fn new() -> Self {
        Self {
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_id: Uuid,
    ) -> Result<BoardSummary, BoardError> {
        self.postgres_board_source
            .lock_summary(executor, user_id, board_id)
            .await?
            .ok_or(BoardError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::boards::data_sources::postgres::board_source::tests::{
        seed_board, seed_user_id,
    };

    #[sqlx::test]
    async fn locks_only_the_owner_s_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_board = seed_board(&db_pool, owner_id, "Mine").await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockBoard::new()
            .execute(&mut *lock_transaction, owner_id, owner_board.id)
            .await
            .expect("the owner should lock their board");
        let foreign_lock = LockBoard::new()
            .execute(&mut *lock_transaction, other_person_id, owner_board.id)
            .await;

        assert_eq!(owner_lock, owner_board);
        assert!(matches!(foreign_lock, Err(BoardError::NotFound)));
    }
}
