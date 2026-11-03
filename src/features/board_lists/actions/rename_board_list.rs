use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_lists::actions::lock_board_of_list::LockBoardOfList;
use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct RenameBoardList<'a> {
    db_pool: &'a PgPool,
    lock_board_of_list: LockBoardOfList,
    build_board: BuildBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> RenameBoardList<'a> {
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
        list_name: &str,
    ) -> Result<Board, BoardListError> {
        let mut rename_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_list
            .execute(&mut rename_transaction, user_id, list_id)
            .await?;
        if !self
            .postgres_board_list_source
            .rename(
                &mut *rename_transaction,
                board_summary.id,
                list_id,
                list_name,
            )
            .await?
        {
            return Err(BoardListError::NotFound);
        }
        let board = self
            .build_board
            .execute(&mut rename_transaction, board_summary)
            .await?;
        rename_transaction.commit().await?;
        Ok(board)
    }
}
