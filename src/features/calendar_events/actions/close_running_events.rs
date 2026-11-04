use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct CloseRunningEvents {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl CloseRunningEvents {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        closing_scope: ClosingScope,
    ) -> Result<(), CalendarEventError> {
        self.postgres_calendar_event_source
            .close_running(&mut *connection, user_id, closing_scope)
            .await?;
        Ok(())
    }
}
