use sqlx::PgPool;
use uuid::Uuid;

use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::Board;
use crate::features::boards::error::BoardError;

pub struct FindBoard<'a> {
    db_pool: &'a PgPool,
    build_board: BuildBoard,
    postgres_board_source: PostgresBoardSource,
}

impl<'a> FindBoard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            build_board: BuildBoard::new(),
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, board_id: Uuid) -> Result<Board, BoardError> {
        let mut board_connection = self.db_pool.acquire().await?;
        let board_summary = self
            .postgres_board_source
            .find_summary(&mut *board_connection, user_id, board_id)
            .await?
            .ok_or(BoardError::NotFound)?;
        self.build_board
            .execute(&mut board_connection, board_summary)
            .await
    }
}
