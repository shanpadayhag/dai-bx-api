use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_lists::actions::lock_board_of_list::LockBoardOfList;
use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct MoveBoardList<'a> {
    db_pool: &'a PgPool,
    lock_board_of_list: LockBoardOfList,
    build_board: BuildBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> MoveBoardList<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_list: LockBoardOfList::new(),
            build_board: BuildBoard::new(),
            postgres_board_list_source: PostgresBoardListSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        requested_position: i32,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardListError> {
        let mut move_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_list
            .execute(&mut move_transaction, user_id, list_id)
            .await?;
        let list_place = self
            .postgres_board_list_source
            .find_place(&mut *move_transaction, board_summary.id, list_id)
            .await?
            .ok_or(BoardListError::NotFound)?;
        let target_position = requested_position.min(list_place.list_count - 1);
        if target_position != list_place.position {
            self.postgres_board_list_source
                .move_to(
                    &mut *move_transaction,
                    board_summary.id,
                    list_id,
                    list_place.position,
                    target_position,
                )
                .await?;
        }
        let board = self
            .build_board
            .execute(
                &mut move_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        move_transaction.commit().await?;
        Ok(board)
    }
}
