use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::error::BoardError;

#[derive(Default)]
pub struct DeleteBoard {
    postgres_board_source: PostgresBoardSource,
}

impl DeleteBoard {
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
    ) -> Result<(), BoardError> {
        if self
            .postgres_board_source
            .delete(executor, user_id, board_id)
            .await?
        {
            Ok(())
        } else {
            Err(BoardError::NotFound)
        }
    }
}
