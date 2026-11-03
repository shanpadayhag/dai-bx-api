use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::{Board, NewBoard};
use crate::features::boards::error::BoardError;

#[derive(Default)]
pub struct CreateBoard {
    postgres_board_source: PostgresBoardSource,
}

impl CreateBoard {
    pub fn new() -> Self {
        Self {
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_name: &str,
    ) -> Result<Board, BoardError> {
        let new_board = NewBoard {
            id: Uuid::now_v7(),
            user_id,
            name: board_name.to_owned(),
        };
        let board_summary = self
            .postgres_board_source
            .insert(executor, &new_board)
            .await?;
        Ok(Board {
            board: board_summary,
            lists: Vec::new(),
            cards: Vec::new(),
            properties: Vec::new(),
        })
    }
}
