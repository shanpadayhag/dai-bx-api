use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct FindEventTaskId {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl FindEventTaskId {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<Uuid>, CalendarEventError> {
        self.postgres_calendar_event_source
            .find_task_id(&mut *connection, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)
    }
}
