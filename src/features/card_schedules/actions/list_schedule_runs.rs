use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::data_sources::postgres::schedule_run_source::PostgresScheduleRunSource;
use crate::features::card_schedules::entities::schedule_run::{RunPage, ScheduleRun};
use crate::features::card_schedules::error::ScheduleError;

pub struct ListScheduleRuns<'a> {
    db_pool: &'a PgPool,
    postgres_card_schedule_source: PostgresCardScheduleSource,
    postgres_schedule_run_source: PostgresScheduleRunSource,
}

impl<'a> ListScheduleRuns<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_card_schedule_source: PostgresCardScheduleSource,
            postgres_schedule_run_source: PostgresScheduleRunSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        schedule_id: Uuid,
        run_page: RunPage,
        now: UtcInstant,
    ) -> Result<Vec<ScheduleRun>, ScheduleError> {
        let card_id = self
            .postgres_card_schedule_source
            .find_card_id(self.db_pool, user_id, schedule_id)
            .await?
            .ok_or(ScheduleError::NotFound)?;
        Ok(self
            .postgres_schedule_run_source
            .list_page(self.db_pool, schedule_id, card_id, run_page, now)
            .await?)
    }
}
