use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::board_properties::data_sources::postgres::board_property_source::PostgresBoardPropertySource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::BoardSummary;

#[derive(Default)]
pub struct LockBoardOfProperty {
    lock_board: LockBoard,
    postgres_board_property_source: PostgresBoardPropertySource,
}

impl LockBoardOfProperty {
    pub fn new() -> Self {
        Self {
            lock_board: LockBoard::new(),
            postgres_board_property_source: PostgresBoardPropertySource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        property_id: Uuid,
    ) -> Result<BoardSummary, BoardPropertyError> {
        let board_id = self
            .postgres_board_property_source
            .find_board_id(&mut *connection, user_id, property_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
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
    use crate::features::board_properties::data_sources::postgres::board_property_source::tests::{
        seed_board_id, seed_property, seed_user_id,
    };
    use crate::features::boards::entities::board::BoardPropertyKind;

    #[sqlx::test]
    async fn locks_the_board_of_only_the_owner_s_property(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Link",
            BoardPropertyKind::Text,
        )
        .await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockBoardOfProperty::new()
            .execute(&mut lock_transaction, owner_id, property_id)
            .await
            .expect("the owner should lock the property's board");
        let foreign_lock = LockBoardOfProperty::new()
            .execute(&mut lock_transaction, other_person_id, property_id)
            .await;

        assert_eq!(owner_lock.id, board_id);
        assert!(matches!(foreign_lock, Err(BoardPropertyError::NotFound)));
    }
}
