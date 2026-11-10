use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::tasks::actions::complete_task_in_transaction::CompleteTaskInTransaction;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct CompleteTask<'a> {
    db_pool: &'a PgPool,
    complete_task_in_transaction: CompleteTaskInTransaction,
}

impl<'a> CompleteTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            complete_task_in_transaction: CompleteTaskInTransaction::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let mut complete_transaction = self.db_pool.begin().await?;
        let group_tasks = self
            .complete_task_in_transaction
            .execute(&mut complete_transaction, user_id, task_id, now)
            .await?;
        complete_transaction.commit().await?;
        Ok(group_tasks)
    }
}
