use sqlx::PgPool;
use uuid::Uuid;

use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct ReopenTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> ReopenTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, task_id: Uuid) -> Result<GroupTasks, TaskError> {
        let mut reopen_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut reopen_transaction, user_id, task_id)
            .await?;
        self.postgres_task_source
            .reopen_task_and_ancestors(&mut *reopen_transaction, task_group_details.id, task_id)
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *reopen_transaction, task_group_details.id)
            .await?;
        reopen_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
