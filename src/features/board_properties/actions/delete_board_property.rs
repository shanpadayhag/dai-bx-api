use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::lock_board_of_property::LockBoardOfProperty;
use crate::features::board_properties::data_sources::postgres::board_property_source::PostgresBoardPropertySource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct DeleteBoardProperty<'a> {
    db_pool: &'a PgPool,
    lock_board_of_property: LockBoardOfProperty,
    build_board: BuildBoard,
    postgres_board_property_source: PostgresBoardPropertySource,
}

impl<'a> DeleteBoardProperty<'a> {
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
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardPropertyError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_property
            .execute(&mut delete_transaction, user_id, property_id)
            .await?;
        let removed_position = self
            .postgres_board_property_source
            .delete(&mut *delete_transaction, board_summary.id, property_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        self.postgres_board_property_source
            .close_position_gap(&mut *delete_transaction, board_summary.id, removed_position)
            .await?;
        let board = self
            .build_board
            .execute(
                &mut delete_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        delete_transaction.commit().await?;
        Ok(board)
    }
}
