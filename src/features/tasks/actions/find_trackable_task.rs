use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::TrackableTask;
use crate::features::tasks::error::TaskError;

#[derive(Default)]
pub struct FindTrackableTask {
    postgres_task_source: PostgresTaskSource,
}

impl FindTrackableTask {
    pub fn new() -> Self {
        Self {
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<TrackableTask, TaskError> {
        self.postgres_task_source
            .find_trackable(&mut *connection, user_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)
    }
}
