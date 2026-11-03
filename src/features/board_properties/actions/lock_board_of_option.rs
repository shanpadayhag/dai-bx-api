use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::BoardSummary;

#[derive(Default)]
pub struct LockBoardOfOption {
    lock_board: LockBoard,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl LockBoardOfOption {
    pub fn new() -> Self {
        Self {
            lock_board: LockBoard::new(),
            postgres_property_option_source: PostgresPropertyOptionSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        option_id: Uuid,
    ) -> Result<BoardSummary, BoardPropertyError> {
        let board_id = self
            .postgres_property_option_source
            .find_board_id(&mut *connection, user_id, option_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        Ok(self
            .lock_board
            .execute(&mut *connection, user_id, board_id)
            .await?)
    }
}
