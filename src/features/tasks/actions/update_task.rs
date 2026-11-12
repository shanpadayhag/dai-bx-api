use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::rename_tracked_events::RenameTrackedEvents;
use crate::features::calendar_events::entities::calendar_event::TrackedSubject;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::{GroupTasks, TaskChanges};
use crate::features::tasks::error::TaskError;

pub struct UpdateTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    rename_tracked_events: RenameTrackedEvents,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> UpdateTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            rename_tracked_events: RenameTrackedEvents::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        task_changes: &TaskChanges,
        renames_events: bool,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
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
        if let Some(task_name) = &task_changes.name
            && renames_events
        {
            self.rename_tracked_events
                .execute(
                    &mut update_transaction,
                    user_id,
                    TrackedSubject::Task(task_id),
                    task_name,
                )
                .await?;
        }
        let uncheck_cutoff_millis = self
            .find_uncheck_cutoffs
            .execute(&mut *update_transaction, user_id, request_time_zone, now)
            .await?
            .cutoff_millis_for(task_group_details.id);
        let group_tasks = self
            .postgres_task_source
            .list_for_group(
                &mut *update_transaction,
                task_group_details.id,
                uncheck_cutoff_millis,
            )
            .await?;
        update_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
