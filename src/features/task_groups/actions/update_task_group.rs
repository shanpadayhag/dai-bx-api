use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::{
    TaskGroup, TaskGroupChanges, TaskGroupSave,
};
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct UpdateTaskGroup {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl UpdateTaskGroup {
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
        task_group_changes: &TaskGroupChanges,
    ) -> Result<TaskGroup, TaskGroupError> {
        match self
            .postgres_task_group_source
            .update(executor, user_id, task_group_id, task_group_changes)
            .await?
        {
            Some(TaskGroupSave::Saved(updated_task_group)) => Ok(updated_task_group),
            Some(TaskGroupSave::CategoryMissing) | None => Err(TaskGroupError::NotFound),
        }
    }
}
