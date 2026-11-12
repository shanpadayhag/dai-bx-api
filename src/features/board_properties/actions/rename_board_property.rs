use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::lock_board_of_property::LockBoardOfProperty;
use crate::features::board_properties::data_sources::postgres::board_property_source::PostgresBoardPropertySource;
use crate::features::board_properties::entities::board_property::NameSave;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct RenameBoardProperty<'a> {
    db_pool: &'a PgPool,
    lock_board_of_property: LockBoardOfProperty,
    build_board: BuildBoard,
    postgres_board_property_source: PostgresBoardPropertySource,
}

impl<'a> RenameBoardProperty<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_property: LockBoardOfProperty::new(),
            build_board: BuildBoard::new(),
            postgres_board_property_source: PostgresBoardPropertySource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        property_id: Uuid,
        property_name: &str,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardPropertyError> {
        let mut rename_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_property
            .execute(&mut rename_transaction, user_id, property_id)
            .await?;
        match self
            .postgres_board_property_source
            .rename(
                &mut *rename_transaction,
                board_summary.id,
                property_id,
                property_name,
            )
            .await?
        {
            NameSave::Saved => {}
            NameSave::Missing => return Err(BoardPropertyError::NotFound),
            NameSave::NameTaken => return Err(BoardPropertyError::PropertyNameTaken),
        }
        let board = self
            .build_board
            .execute(
                &mut rename_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        rename_transaction.commit().await?;
        Ok(board)
    }
}
