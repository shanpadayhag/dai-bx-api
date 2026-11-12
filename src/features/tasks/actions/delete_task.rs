use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct DeleteTask<'a> {
    db_pool: &'a PgPool,
    lock_group_of_task: LockGroupOfTask,
    close_running_events: CloseRunningEvents,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> DeleteTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_group_of_task: LockGroupOfTask::new(),
            close_running_events: CloseRunningEvents::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut delete_transaction, user_id, task_id)
            .await?;
        self.close_running_events
            .execute(
                &mut delete_transaction,
                user_id,
                ClosingScope::Task(task_id),
            )
            .await?;
        let task_place = self
            .postgres_task_source
            .delete(&mut *delete_transaction, task_group_details.id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        self.postgres_task_source
            .close_position_gap(&mut *delete_transaction, task_group_details.id, task_place)
            .await?;
        let uncheck_cutoff_millis = self
            .find_uncheck_cutoffs
            .execute(&mut *delete_transaction, user_id, request_time_zone, now)
            .await?
            .cutoff_millis_for(task_group_details.id);
        let group_tasks = self
            .postgres_task_source
            .list_for_group(
                &mut *delete_transaction,
                task_group_details.id,
                uncheck_cutoff_millis,
            )
            .await?;
        delete_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
