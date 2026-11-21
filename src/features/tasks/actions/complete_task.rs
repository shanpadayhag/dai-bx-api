use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::activity::actions::record_activity::RecordActivity;
use crate::features::activity::entities::activity_entry::{
    ActivityDetails, ActivitySubject, NewActivityEntry,
};
use crate::features::tasks::actions::complete_task_in_transaction::CompleteTaskInTransaction;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct CompleteTask<'a> {
    db_pool: &'a PgPool,
    complete_task_in_transaction: CompleteTaskInTransaction,
    record_activity: RecordActivity,
}

impl<'a> CompleteTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            complete_task_in_transaction: CompleteTaskInTransaction::new(),
            record_activity: RecordActivity::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let mut complete_transaction = self.db_pool.begin().await?;
        let group_tasks = self
            .complete_task_in_transaction
            .execute(
                &mut complete_transaction,
                user_id,
                task_id,
                request_time_zone,
                now,
            )
            .await?;
        self.record_activity
            .execute(
                &mut *complete_transaction,
                NewActivityEntry {
                    user_id,
                    occurred_at: now,
                    subject: ActivitySubject::Task(task_id),
                    details: ActivityDetails::TaskCompleted {
                        is_subtask: is_subtask_in(&group_tasks, task_id),
                    },
                },
            )
            .await?;
        complete_transaction.commit().await?;
        Ok(group_tasks)
    }
}

fn is_subtask_in(group_tasks: &GroupTasks, task_id: Uuid) -> bool {
    group_tasks
        .tasks
        .iter()
        .any(|task| task.id == task_id && task.parent_id.is_some())
}
