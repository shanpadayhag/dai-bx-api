use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::TaskGroup;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct ListTaskGroups {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl ListTaskGroups {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<TaskGroup>, TaskGroupError> {
        Ok(self
            .postgres_task_group_source
            .list_for_user(executor, user_id)
            .await?)
    }
}
