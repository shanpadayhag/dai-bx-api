use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::BoardSummary;
use crate::features::boards::error::BoardError;

#[derive(Default)]
pub struct ListBoards {
    postgres_board_source: PostgresBoardSource,
}

impl ListBoards {
    pub fn new() -> Self {
        Self {
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<BoardSummary>, BoardError> {
        Ok(self
            .postgres_board_source
            .list_for_user(executor, user_id)
            .await?)
    }
}
