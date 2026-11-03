use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_properties::actions::lock_board_of_option::LockBoardOfOption;
use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct DeletePropertyOption<'a> {
    db_pool: &'a PgPool,
    lock_board_of_option: LockBoardOfOption,
    build_board: BuildBoard,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl<'a> DeletePropertyOption<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_option: LockBoardOfOption::new(),
            build_board: BuildBoard::new(),
            postgres_property_option_source: PostgresPropertyOptionSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        option_id: Uuid,
    ) -> Result<Board, BoardPropertyError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_option
            .execute(&mut delete_transaction, user_id, option_id)
            .await?;
        let option_place = self
            .postgres_property_option_source
            .find_place(&mut *delete_transaction, board_summary.id, option_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        if option_place.option_count <= 1 {
            return Err(BoardPropertyError::Invalid(String::from(
                "a choice property keeps at least one option",
            )));
        }
        self.postgres_property_option_source
            .delete(
                &mut *delete_transaction,
                option_place.property_id,
                option_id,
            )
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        self.postgres_property_option_source
            .close_position_gap(
                &mut *delete_transaction,
                option_place.property_id,
                option_place.position,
            )
            .await?;
        let board = self
            .build_board
            .execute(&mut delete_transaction, board_summary)
            .await?;
        delete_transaction.commit().await?;
        Ok(board)
    }
}
