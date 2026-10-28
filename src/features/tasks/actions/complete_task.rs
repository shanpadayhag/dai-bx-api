use sqlx::PgPool;
use uuid::Uuid;

use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct CompleteTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> CompleteTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, task_id: Uuid) -> Result<GroupTasks, TaskError> {
        let mut complete_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut complete_transaction, user_id, task_id)
            .await?;
        self.postgres_task_source
            .complete_task_and_descendants(
                &mut *complete_transaction,
                task_group_details.id,
                task_id,
            )
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *complete_transaction, task_group_details.id)
            .await?;
        complete_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
