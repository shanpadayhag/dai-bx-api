use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_properties::actions::lock_board_of_property::LockBoardOfProperty;
use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::entities::board_property::{
    NameSave, NewPropertyOption, OptionDraft, PROPERTY_OPTION_LIMIT,
};
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::{Board, BoardPropertyKind};

pub struct CreatePropertyOption<'a> {
    db_pool: &'a PgPool,
    lock_board_of_property: LockBoardOfProperty,
    build_board: BuildBoard,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl<'a> CreatePropertyOption<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_property: LockBoardOfProperty::new(),
            build_board: BuildBoard::new(),
            postgres_property_option_source: PostgresPropertyOptionSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        property_id: Uuid,
        option_draft: &OptionDraft,
    ) -> Result<Board, BoardPropertyError> {
        let mut create_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_property
            .execute(&mut create_transaction, user_id, property_id)
            .await?;
        let property_option_count = self
            .postgres_property_option_source
            .find_option_count(&mut *create_transaction, board_summary.id, property_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        if property_option_count.kind != BoardPropertyKind::Choice {
            return Err(BoardPropertyError::Invalid(String::from(
                "only a choice property has options",
            )));
        }
        if property_option_count.option_count >= PROPERTY_OPTION_LIMIT {
            return Err(BoardPropertyError::Invalid(String::from(
                "a choice property has at most 50 options",
            )));
        }
        let option_save = self
            .postgres_property_option_source
            .insert_last(
                &mut *create_transaction,
                &NewPropertyOption {
                    id: Uuid::now_v7(),
                    user_id,
                    property_id,
                    name: option_draft.name.clone(),
                    color: option_draft.color.clone(),
                },
            )
            .await?;
        if option_save == NameSave::NameTaken {
            return Err(BoardPropertyError::OptionNameTaken);
        }
        let board = self
            .build_board
            .execute(&mut create_transaction, board_summary)
            .await?;
        create_transaction.commit().await?;
        Ok(board)
    }
}
