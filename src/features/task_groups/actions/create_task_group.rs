use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::{NewTaskGroup, TaskGroup};
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct CreateTaskGroup {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl CreateTaskGroup {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_name: &str,
    ) -> Result<TaskGroup, TaskGroupError> {
        let new_task_group = NewTaskGroup {
            id: Uuid::now_v7(),
            user_id,
            name: task_group_name.to_owned(),
        };
        Ok(self
            .postgres_task_group_source
            .insert(executor, &new_task_group)
            .await?)
    }
}
