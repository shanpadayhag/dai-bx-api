use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::entities::board_list::NewBoardList;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::Board;

pub struct CreateBoardList<'a> {
    db_pool: &'a PgPool,
    lock_board: LockBoard,
    build_board: BuildBoard,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> CreateBoardList<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board: LockBoard::new(),
            build_board: BuildBoard::new(),
            postgres_board_list_source: PostgresBoardListSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        list_name: &str,
    ) -> Result<Board, BoardListError> {
        let mut create_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board
            .execute(&mut *create_transaction, user_id, board_id)
            .await?;
        self.postgres_board_list_source
            .insert_last(
                &mut *create_transaction,
                &NewBoardList {
                    id: Uuid::now_v7(),
                    user_id,
                    board_id,
                    name: list_name.to_owned(),
                },
            )
            .await?;
        let board = self
            .build_board
            .execute(&mut create_transaction, board_summary)
            .await?;
        create_transaction.commit().await?;
        Ok(board)
    }
}
