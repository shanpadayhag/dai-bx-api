use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::TrackableTask;
use crate::features::tasks::error::TaskError;

#[derive(Default)]
pub struct FindTrackableTask {
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl FindTrackableTask {
    pub fn new() -> Self {
        Self {
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<TrackableTask, TaskError> {
        let uncheck_cutoffs = self
            .find_uncheck_cutoffs
            .execute(&mut *connection, user_id, request_time_zone, now)
            .await?;
        self.postgres_task_source
            .find_trackable(&mut *connection, user_id, task_id, &uncheck_cutoffs)
            .await?
            .ok_or(TaskError::NotFound)
    }
}
