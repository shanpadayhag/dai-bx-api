use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::data_sources::postgres::board_property_source::PostgresBoardPropertySource;
use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::entities::board_property::{
    BOARD_PROPERTY_LIMIT, NameSave, NewBoardProperty, NewPropertyOption, PropertyDraft,
};
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::entities::board::Board;

pub struct CreateBoardProperty<'a> {
    db_pool: &'a PgPool,
    lock_board: LockBoard,
    build_board: BuildBoard,
    postgres_board_property_source: PostgresBoardPropertySource,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl<'a> CreateBoardProperty<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board: LockBoard::new(),
            build_board: BuildBoard::new(),
            postgres_board_property_source: PostgresBoardPropertySource,
            postgres_property_option_source: PostgresPropertyOptionSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        property_draft: &PropertyDraft,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardPropertyError> {
        let mut create_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board
            .execute(&mut *create_transaction, user_id, board_id)
            .await?;
        let property_count = self
            .postgres_board_property_source
            .count_on_board(&mut *create_transaction, board_id)
            .await?;
        if property_count >= BOARD_PROPERTY_LIMIT {
            return Err(BoardPropertyError::Invalid(String::from(
                "a board has at most 20 properties",
            )));
        }
        let property_id = Uuid::now_v7();
        let property_save = self
            .postgres_board_property_source
            .insert_last(
                &mut *create_transaction,
                &NewBoardProperty {
                    id: property_id,
                    user_id,
                    board_id,
                    name: property_draft.name.clone(),
                    kind: property_draft.kind,
                },
            )
            .await?;
        if property_save == NameSave::NameTaken {
            return Err(BoardPropertyError::PropertyNameTaken);
        }
        if !property_draft.option_drafts.is_empty() {
            let new_property_options: Vec<NewPropertyOption> = property_draft
                .option_drafts
                .iter()
                .map(|option_draft| NewPropertyOption {
                    id: Uuid::now_v7(),
                    user_id,
                    property_id,
                    name: option_draft.name.clone(),
                    color: option_draft.color.clone(),
                })
                .collect();
            let options_save = self
                .postgres_property_option_source
                .insert_for_new_property(
                    &mut *create_transaction,
                    user_id,
                    property_id,
                    &new_property_options,
                )
                .await?;
            if options_save == NameSave::NameTaken {
                return Err(BoardPropertyError::OptionNameTaken);
            }
        }
        let board = self
            .build_board
            .execute(
                &mut create_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        create_transaction.commit().await?;
        Ok(board)
    }
}
