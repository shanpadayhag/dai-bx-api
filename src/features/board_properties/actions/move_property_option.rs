use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::lock_board_of_option::LockBoardOfOption;
use crate::features::board_properties::data_sources::postgres::property_option_source::PostgresPropertyOptionSource;
use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;

pub struct MovePropertyOption<'a> {
    db_pool: &'a PgPool,
    lock_board_of_option: LockBoardOfOption,
    build_board: BuildBoard,
    postgres_property_option_source: PostgresPropertyOptionSource,
}

impl<'a> MovePropertyOption<'a> {
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
        requested_position: i32,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Board, BoardPropertyError> {
        let mut move_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_option
            .execute(&mut move_transaction, user_id, option_id)
            .await?;
        let option_place = self
            .postgres_property_option_source
            .find_place(&mut *move_transaction, board_summary.id, option_id)
            .await?
            .ok_or(BoardPropertyError::NotFound)?;
        let target_position = requested_position.min(option_place.option_count - 1);
        if target_position != option_place.position {
            self.postgres_property_option_source
                .move_to(
                    &mut *move_transaction,
                    option_place.property_id,
                    option_id,
                    option_place.position,
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
