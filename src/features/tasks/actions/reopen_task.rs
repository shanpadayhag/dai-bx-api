use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::activity::actions::record_activity::RecordActivity;
use crate::features::activity::entities::activity_entry::{
    ActivityDetails, ActivitySubject, NewActivityEntry,
};
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct ReopenTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
    record_activity: RecordActivity,
}

impl<'a> ReopenTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
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
        let mut reopen_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut reopen_transaction, user_id, task_id)
            .await?;
        let uncheck_cutoff_millis = self
            .find_uncheck_cutoffs
            .execute(&mut *reopen_transaction, user_id, request_time_zone, now)
            .await?
            .cutoff_millis_for(task_group_details.id);
        self.postgres_task_source
            .reopen_task_and_ancestors(
                &mut *reopen_transaction,
                task_group_details.id,
                task_id,
                uncheck_cutoff_millis,
            )
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(
                &mut *reopen_transaction,
                task_group_details.id,
                uncheck_cutoff_millis,
            )
            .await?;
        self.record_activity
            .execute(
                &mut *reopen_transaction,
                NewActivityEntry {
                    user_id,
                    occurred_at: now,
                    subject: ActivitySubject::Task(task_id),
                    details: ActivityDetails::TaskReopened {
                        is_subtask: group_tasks
                            .iter()
                            .any(|task| task.id == task_id && task.parent_id.is_some()),
                    },
                },
            )
            .await?;
        reopen_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
