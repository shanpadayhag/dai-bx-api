use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventKind, EventRange,
};
use crate::features::calendar_events::error::CalendarEventError;

pub struct ListPlannedStarts<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl<'a> ListPlannedStarts<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
            postgres_event_series_source: PostgresEventSeriesSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        event_range: EventRange,
    ) -> Result<Vec<CalendarEvent>, CalendarEventError> {
        let (range_events, shown_series) = tokio::try_join!(
            self.postgres_calendar_event_source
                .list_in_range(self.db_pool, user_id, event_range),
            self.postgres_event_series_source.list_shown_in_range(
                self.db_pool,
                user_id,
                event_range
            ),
        )?;
        let mut planned_starts: Vec<CalendarEvent> = range_events
            .into_iter()
            .chain(
                shown_series
                    .iter()
                    .flat_map(|series| series.occurrences_in(event_range)),
            )
            .filter(|range_event| {
                range_event.kind == CalendarEventKind::Planned
                    && event_range.from <= range_event.starts_at
                    && range_event.starts_at < event_range.to
            })
            .collect();
        planned_starts.sort_by_key(|planned_start| (planned_start.starts_at, planned_start.id));
        Ok(planned_starts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, new_event, seed_event, seed_user_id,
    };
    use crate::features::calendar_events::data_sources::postgres::event_series_source::tests::{
        new_weekly_series, seed_series,
    };
    use crate::features::calendar_events::entities::calendar_event::NewCalendarEvent;

    #[sqlx::test]
    async fn lists_only_planned_events_and_occurrences_that_start_in_the_range(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let starting_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-09T09:58:00Z"),
                ends_at: Some(instant("2026-11-09T11:00:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;
        seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-09T09:00:00Z"),
                ends_at: Some(instant("2026-11-09T11:00:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;
        seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-09T09:59:00Z"),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-09T09:59:00Z"),
                ends_at: Some(instant("2026-11-09T11:00:00Z")),
                ..new_event(other_user_id, CalendarEventKind::Planned)
            },
        )
        .await;

        let planned_starts = ListPlannedStarts::new(&db_pool)
            .execute(
                user_id,
                EventRange {
                    from: instant("2026-11-09T09:55:00Z"),
                    to: instant("2026-11-09T10:00:00.001Z"),
                },
            )
            .await
            .expect("the list should run");

        let listed_ids: Vec<Uuid> = planned_starts
            .iter()
            .map(|planned_start| planned_start.id)
            .collect();
        assert_eq!(listed_ids, vec![starting_event_id, series_id]);
        assert_eq!(planned_starts[1].starts_at, instant("2026-11-09T10:00:00Z"));
    }
}
