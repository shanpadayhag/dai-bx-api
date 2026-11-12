use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::Board;
use crate::features::boards::error::BoardError;

pub struct UpdateBoard<'a> {
    db_pool: &'a PgPool,
    lock_board: LockBoard,
    build_board: BuildBoard,
    postgres_board_source: PostgresBoardSource,
}

impl<'a> UpdateBoard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board: LockBoard::new(),
            build_board: BuildBoard::new(),
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        board_name: &str,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardError> {
        let mut update_transaction = self.db_pool.begin().await?;
        self.lock_board
            .execute(&mut *update_transaction, user_id, board_id)
            .await?;
        let renamed_board = self
            .postgres_board_source
            .rename(&mut *update_transaction, board_id, board_name)
            .await?;
        let board = self
            .build_board
            .execute(
                &mut update_transaction,
                user_id,
                renamed_board,
                request_time_zone,
                now,
            )
            .await?;
        update_transaction.commit().await?;
        Ok(board)
    }
}
