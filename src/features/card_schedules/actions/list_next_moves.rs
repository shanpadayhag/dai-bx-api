use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::boards::entities::board::NextMove;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct ListNextMoves {
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl ListNextMoves {
    pub fn new() -> Self {
        Self {
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        board_id: Uuid,
    ) -> Result<HashMap<Uuid, NextMove>, ScheduleError> {
        Ok(self
            .postgres_card_schedule_source
            .list_next_moves(&mut *connection, board_id)
            .await?
            .into_iter()
            .collect())
    }
}
