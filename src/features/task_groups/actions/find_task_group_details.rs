use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct FindTaskGroupDetails {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl FindTaskGroupDetails {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<TaskGroupDetails, TaskGroupError> {
        self.postgres_task_group_source
            .find_details(executor, user_id, task_group_id)
            .await?
            .ok_or(TaskGroupError::NotFound)
    }
}
