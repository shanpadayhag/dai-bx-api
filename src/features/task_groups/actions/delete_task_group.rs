use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct DeleteTaskGroup {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl DeleteTaskGroup {
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
    ) -> Result<(), TaskGroupError> {
        if self
            .postgres_task_group_source
            .delete(executor, user_id, task_group_id)
            .await?
        {
            Ok(())
        } else {
            Err(TaskGroupError::NotFound)
        }
    }
}
