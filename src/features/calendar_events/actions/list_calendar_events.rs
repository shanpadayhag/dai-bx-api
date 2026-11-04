use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::{CalendarEvent, EventRange};
use crate::features::calendar_events::error::CalendarEventError;

pub struct ListCalendarEvents<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl<'a> ListCalendarEvents<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        event_range: EventRange,
    ) -> Result<Vec<CalendarEvent>, CalendarEventError> {
        Ok(self
            .postgres_calendar_event_source
            .list_in_range(self.db_pool, user_id, event_range)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, new_event, seed_event, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::CalendarEventKind;

    #[sqlx::test]
    async fn lists_the_person_s_events_in_the_range(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let listed_events = ListCalendarEvents::new(&db_pool)
            .execute(
                user_id,
                EventRange {
                    from: instant("2026-11-02T00:00:00Z"),
                    to: instant("2026-11-09T00:00:00Z"),
                },
            )
            .await
            .expect("the list should run");

        let listed_ids: Vec<Uuid> = listed_events
            .iter()
            .map(|listed_event| listed_event.id)
            .collect();
        assert_eq!(listed_ids, vec![event_id]);
    }
}
