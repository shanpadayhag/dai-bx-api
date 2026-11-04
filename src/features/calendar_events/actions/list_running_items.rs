use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::RunningItem;
use crate::features::calendar_events::error::CalendarEventError;

pub struct ListRunningItems<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl<'a> ListRunningItems<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<Vec<RunningItem>, CalendarEventError> {
        Ok(self
            .postgres_calendar_event_source
            .list_running(self.db_pool, user_id)
            .await?)
    }
}
