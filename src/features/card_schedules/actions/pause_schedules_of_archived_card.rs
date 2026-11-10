use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::data_sources::postgres::schedule_block_source::PostgresScheduleBlockSource;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct PauseSchedulesOfArchivedCard {
    postgres_card_schedule_source: PostgresCardScheduleSource,
    postgres_schedule_block_source: PostgresScheduleBlockSource,
}

impl PauseSchedulesOfArchivedCard {
    pub fn new() -> Self {
        Self {
            postgres_card_schedule_source: PostgresCardScheduleSource,
            postgres_schedule_block_source: PostgresScheduleBlockSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        card_id: Uuid,
        now: UtcInstant,
    ) -> Result<(), ScheduleError> {
        let paused_schedule_ids = self
            .postgres_card_schedule_source
            .pause_for_archive(&mut *connection, card_id)
            .await?;
        if paused_schedule_ids.is_empty() {
            return Ok(());
        }
        self.postgres_schedule_block_source
            .set_series_hidden(&mut *connection, &paused_schedule_ids, true)
            .await?;
        self.postgres_schedule_block_source
            .delete_future_events(&mut *connection, &paused_schedule_ids, now)
            .await?;
        Ok(())
    }
}
