use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::BoardSummary;

#[derive(Default)]
pub struct LockBoardOfList {
    lock_board: LockBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl LockBoardOfList {
    pub fn new() -> Self {
        Self {
            lock_board: LockBoard::new(),
            postgres_board_list_source: PostgresBoardListSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<BoardSummary, BoardListError> {
        let board_id = self
            .postgres_board_list_source
            .find_board_id(&mut *connection, user_id, list_id)
            .await?
            .ok_or(BoardListError::NotFound)?;
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
    use crate::features::board_lists::data_sources::postgres::board_list_source::tests::{
        seed_board_id, seed_list, seed_user_id,
    };

    #[sqlx::test]
    async fn locks_the_board_of_only_the_owner_s_list(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let list_id = seed_list(&db_pool, owner_id, board_id, "Backlog").await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockBoardOfList::new()
            .execute(&mut lock_transaction, owner_id, list_id)
            .await
            .expect("the owner should lock the list's board");
        let foreign_lock = LockBoardOfList::new()
            .execute(&mut lock_transaction, other_person_id, list_id)
            .await;

        assert_eq!(owner_lock.id, board_id);
        assert!(matches!(foreign_lock, Err(BoardListError::NotFound)));
    }
}
