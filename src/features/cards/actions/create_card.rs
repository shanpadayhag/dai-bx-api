use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_lists::actions::lock_board_of_list::LockBoardOfList;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card::{NEW_CARD_NOTES, NewCard};
use crate::features::cards::error::CardError;

pub struct CreateCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_list: LockBoardOfList,
    build_board: BuildBoard,
    postgres_card_source: PostgresCardSource,
}

impl<'a> CreateCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_list: LockBoardOfList::new(),
            build_board: BuildBoard::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        card_title: &str,
    ) -> Result<Board, CardError> {
        let mut create_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_list
            .execute(&mut create_transaction, user_id, list_id)
            .await?;
        self.postgres_card_source
            .insert_last(
                &mut *create_transaction,
                &NewCard {
                    id: Uuid::now_v7(),
                    user_id,
                    board_id: board_summary.id,
                    list_id,
                    title: card_title.to_owned(),
                },
                NEW_CARD_NOTES,
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
