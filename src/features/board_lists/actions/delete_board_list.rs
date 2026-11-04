use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_lists::actions::lock_board_of_list::LockBoardOfList;
use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;

pub struct DeleteBoardList<'a> {
    db_pool: &'a PgPool,
    lock_board_of_list: LockBoardOfList,
    close_running_events: CloseRunningEvents,
    build_board: BuildBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> DeleteBoardList<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_list: LockBoardOfList::new(),
            close_running_events: CloseRunningEvents::new(),
            build_board: BuildBoard::new(),
            postgres_board_list_source: PostgresBoardListSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, list_id: Uuid) -> Result<Board, BoardListError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_list
            .execute(&mut delete_transaction, user_id, list_id)
            .await?;
        self.close_running_events
            .execute(
                &mut delete_transaction,
                user_id,
                ClosingScope::BoardList(list_id),
            )
            .await?;
        let removed_position = self
            .postgres_board_list_source
            .delete(&mut *delete_transaction, board_summary.id, list_id)
            .await?
            .ok_or(BoardListError::NotFound)?;
        self.postgres_board_list_source
            .close_position_gap(&mut *delete_transaction, board_summary.id, removed_position)
            .await?;
        let board = self
            .build_board
            .execute(&mut delete_transaction, board_summary)
            .await?;
        delete_transaction.commit().await?;
        Ok(board)
    }
}
