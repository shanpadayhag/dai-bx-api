use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_properties::actions::lock_board_of_option::LockBoardOfOption;
use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::entities::board_property::{NameSave, OptionChanges};
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct UpdatePropertyOption<'a> {
    db_pool: &'a PgPool,
    lock_board_of_option: LockBoardOfOption,
    build_board: BuildBoard,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl<'a> UpdatePropertyOption<'a> {
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
        option_changes: &OptionChanges,
    ) -> Result<Board, BoardPropertyError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_option
            .execute(&mut update_transaction, user_id, option_id)
            .await?;
        match self
            .postgres_property_option_source
            .update(
                &mut *update_transaction,
                board_summary.id,
                option_id,
                option_changes,
            )
            .await?
        {
            NameSave::Saved => {}
            NameSave::Missing => return Err(BoardPropertyError::NotFound),
            NameSave::NameTaken => return Err(BoardPropertyError::OptionNameTaken),
        }
        let board = self
            .build_board
            .execute(&mut update_transaction, board_summary)
            .await?;
        update_transaction.commit().await?;
        Ok(board)
    }
}
