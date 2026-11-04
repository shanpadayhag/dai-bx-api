use sqlx::PgConnection;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::{
    NewRunningEvent, RunningItem, RunningOpen,
};
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct OpenRunningEvent {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl OpenRunningEvent {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        new_running_event: &NewRunningEvent,
    ) -> Result<RunningItem, CalendarEventError> {
        let running_open = self
            .postgres_calendar_event_source
            .open_running(&mut *connection, new_running_event)
            .await?;
        if running_open == RunningOpen::AlreadyRunning {
            return Err(CalendarEventError::AlreadyRunning);
        }
        self.postgres_calendar_event_source
            .find_running_item(
                &mut *connection,
                new_running_event.user_id,
                new_running_event.id,
            )
            .await?
            .ok_or(CalendarEventError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        new_running_event, seed_task_ids, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::TrackedSubject;

    #[sqlx::test]
    async fn opens_a_running_item_once_per_task(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let mut first_connection = db_pool.acquire().await.expect("a connection");
        let mut second_connection = db_pool.acquire().await.expect("a connection");

        let running_item = OpenRunningEvent::new()
            .execute(
                &mut first_connection,
                &new_running_event(
                    user_id,
                    "Write the changelog",
                    TrackedSubject::Task(task_id),
                ),
            )
            .await
            .expect("the first start should open");
        let second_open = OpenRunningEvent::new()
            .execute(
                &mut second_connection,
                &new_running_event(
                    user_id,
                    "Write the changelog",
                    TrackedSubject::Task(task_id),
                ),
            )
            .await;

        assert_eq!(running_item.event.title, "Write the changelog");
        assert!(running_item.event.is_running());
        assert!(matches!(
            second_open,
            Err(CalendarEventError::AlreadyRunning)
        ));
    }
}
