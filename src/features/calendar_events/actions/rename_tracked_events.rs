use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::TrackedSubject;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct RenameTrackedEvents {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl RenameTrackedEvents {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        tracked_subject: TrackedSubject,
        title: &str,
    ) -> Result<(), CalendarEventError> {
        self.postgres_calendar_event_source
            .rename_tracked(&mut *connection, user_id, tracked_subject, title)
            .await?;
        Ok(())
    }
}
