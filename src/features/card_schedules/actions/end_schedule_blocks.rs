use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::data_sources::postgres::schedule_block_source::PostgresScheduleBlockSource;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct EndScheduleBlocks {
    postgres_schedule_block_source: PostgresScheduleBlockSource,
}

impl EndScheduleBlocks {
    pub fn new() -> Self {
        Self {
            postgres_schedule_block_source: PostgresScheduleBlockSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        schedule_ids: &[Uuid],
        now: UtcInstant,
    ) -> Result<(), ScheduleError> {
        self.postgres_schedule_block_source
            .end_series(&mut *connection, schedule_ids, now)
            .await?;
        self.postgres_schedule_block_source
            .delete_future_events(&mut *connection, schedule_ids, now)
            .await?;
        self.postgres_schedule_block_source
            .release_events(&mut *connection, schedule_ids)
            .await?;
        Ok(())
    }
}
