use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::task_groups::actions::lock_task_group::LockTaskGroup;
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::error::TaskError;

#[derive(Default)]
pub struct LockGroupOfTask {
    lock_task_group: LockTaskGroup,
    postgres_task_source: PostgresTaskSource,
}

impl LockGroupOfTask {
    pub fn new() -> Self {
        Self {
            lock_task_group: LockTaskGroup::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<TaskGroupDetails, TaskError> {
        let task_group_id = self
            .postgres_task_source
            .find_group_id(&mut *connection, user_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        Ok(self
            .lock_task_group
            .execute(&mut *connection, user_id, task_group_id)
            .await?)
    }
}
