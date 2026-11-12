use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
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

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardError> {
        let mut board_connection = self.db_pool.acquire().await?;
        let board_summary = self
            .postgres_board_source
            .find_summary(&mut *board_connection, user_id, board_id)
            .await?
            .ok_or(BoardError::NotFound)?;
        self.build_board
            .execute(
                &mut board_connection,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await
    }
}
