use sqlx::PgPool;
use uuid::Uuid;

use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::{GroupTasks, TaskChanges};
use crate::features::tasks::error::TaskError;

pub struct UpdateTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> UpdateTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        task_changes: &TaskChanges,
    ) -> Result<GroupTasks, TaskError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut update_transaction, user_id, task_id)
            .await?;
        if !self
            .postgres_task_source
            .update(
                &mut *update_transaction,
                task_group_details.id,
                task_id,
                task_changes,
            )
            .await?
        {
            return Err(TaskError::NotFound);
        }
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *update_transaction, task_group_details.id)
            .await?;
        update_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
