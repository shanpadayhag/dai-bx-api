use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::cards::actions::fire_card_trigger::FireCardTrigger;
use crate::features::cards::entities::card_trigger::TriggerEvent;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::{CompletedLinkedTask, GroupTasks};
use crate::features::tasks::error::TaskError;

fn finishing_task_per_card(
    completed_task_id: Uuid,
    mut completed_linked_tasks: Vec<CompletedLinkedTask>,
) -> Vec<CompletedLinkedTask> {
    completed_linked_tasks.sort_by_key(|completed_linked_task| {
        (
            completed_linked_task.card_id,
            completed_linked_task.task_id != completed_task_id,
            completed_linked_task.task_id,
        )
    });
    completed_linked_tasks.dedup_by_key(|completed_linked_task| completed_linked_task.card_id);
    completed_linked_tasks
}

#[derive(Default)]
pub struct CompleteTaskInTransaction {
    lock_group_of_task: LockGroupOfTask,
    close_running_events: CloseRunningEvents,
    fire_card_trigger: FireCardTrigger,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl CompleteTaskInTransaction {
    pub fn new() -> Self {
        Self {
            lock_group_of_task: LockGroupOfTask::new(),
            close_running_events: CloseRunningEvents::new(),
            fire_card_trigger: FireCardTrigger::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut *connection, user_id, task_id)
            .await?;
        self.close_running_events
            .execute(&mut *connection, user_id, ClosingScope::Task(task_id))
            .await?;
        let uncheck_cutoffs = self
            .find_uncheck_cutoffs
            .execute(&mut *connection, user_id, request_time_zone, now)
            .await?;
        let uncheck_cutoff_millis = uncheck_cutoffs.cutoff_millis_for(task_group_details.id);
        let completed_linked_tasks = self
            .postgres_task_source
            .complete_task_and_descendants(
                &mut *connection,
                task_group_details.id,
                task_id,
                uncheck_cutoff_millis,
            )
            .await?;
        for finishing_task in finishing_task_per_card(task_id, completed_linked_tasks) {
            self.fire_card_trigger
                .execute(
                    &mut *connection,
                    user_id,
                    finishing_task.card_id,
                    TriggerEvent::Finished {
                        uncheck_cutoffs: &uncheck_cutoffs,
                    },
                    Some(&finishing_task.name),
                    now,
                )
                .await?;
        }
        let group_tasks = self
            .postgres_task_source
            .list_for_group(
                &mut *connection,
                task_group_details.id,
                uncheck_cutoff_millis,
            )
            .await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
