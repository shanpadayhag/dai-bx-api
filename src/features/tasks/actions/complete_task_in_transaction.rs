use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

#[derive(Default)]
pub struct CompleteTaskInTransaction {
    lock_group_of_task: LockGroupOfTask,
    close_running_events: CloseRunningEvents,
    postgres_task_source: PostgresTaskSource,
}

impl CompleteTaskInTransaction {
    pub fn new() -> Self {
        Self {
            lock_group_of_task: LockGroupOfTask::new(),
            close_running_events: CloseRunningEvents::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<GroupTasks, TaskError> {
        let task_group_details = self
            .lock_group_of_task
            .execute(&mut *connection, user_id, task_id)
            .await?;
        self.close_running_events
            .execute(&mut *connection, user_id, ClosingScope::Task(task_id))
            .await?;
        self.postgres_task_source
            .complete_task_and_descendants(&mut *connection, task_group_details.id, task_id)
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *connection, task_group_details.id)
            .await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
