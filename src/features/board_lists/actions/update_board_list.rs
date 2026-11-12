use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_lists::actions::lock_board_of_list::LockBoardOfList;
use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::entities::board_list::ListChanges;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct UpdateBoardList<'a> {
    db_pool: &'a PgPool,
    lock_board_of_list: LockBoardOfList,
    build_board: BuildBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> UpdateBoardList<'a> {
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
        list_changes: ListChanges,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardListError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_list
            .execute(&mut update_transaction, user_id, list_id)
            .await?;
        if let Some(list_name) = list_changes.name.as_deref()
            && !self
                .postgres_board_list_source
                .rename(
                    &mut *update_transaction,
                    board_summary.id,
                    list_id,
                    list_name,
                )
                .await?
        {
            return Err(BoardListError::NotFound);
        }
        if let Some(archive_after) = list_changes.archive_after_change
            && !self
                .postgres_board_list_source
                .set_archive_after(
                    &mut *update_transaction,
                    board_summary.id,
                    list_id,
                    archive_after,
                )
                .await?
        {
            return Err(BoardListError::NotFound);
        }
        let board = self
            .build_board
            .execute(
                &mut update_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        update_transaction.commit().await?;
        Ok(board)
    }
}
