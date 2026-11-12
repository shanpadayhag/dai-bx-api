use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::lock_board_of_property::LockBoardOfProperty;
use crate::features::board_properties::data_sources::postgres::board_property_source::PostgresBoardPropertySource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct MoveBoardProperty<'a> {
    db_pool: &'a PgPool,
    lock_board_of_property: LockBoardOfProperty,
    build_board: BuildBoard,
    postgres_board_property_source: PostgresBoardPropertySource,
}

impl<'a> MoveBoardProperty<'a> {
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
        requested_position: i32,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardPropertyError> {
        let mut move_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_property
            .execute(&mut move_transaction, user_id, property_id)
            .await?;
        let property_place = self
            .postgres_board_property_source
            .find_place(&mut *move_transaction, board_summary.id, property_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        let target_position = requested_position.min(property_place.property_count - 1);
        if target_position != property_place.position {
            self.postgres_board_property_source
                .move_to(
                    &mut *move_transaction,
                    board_summary.id,
                    property_id,
                    property_place.position,
                    target_position,
                )
                .await?;
        }
        let board = self
            .build_board
            .execute(
                &mut move_transaction,
                user_id,
                board_summary,
                request_time_zone,
                now,
            )
            .await?;
        move_transaction.commit().await?;
        Ok(board)
    }
}
