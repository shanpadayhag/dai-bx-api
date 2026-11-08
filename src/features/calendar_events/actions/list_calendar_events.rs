use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::calendar_event::{CalendarRange, EventRange};
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::card_schedules::actions::list_scheduled_moves::ListScheduledMoves;

pub struct ListCalendarEvents<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl<'a> ListCalendarEvents<'a> {
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
    ) -> Result<CalendarRange, CalendarEventError> {
        let (mut range_events, shown_series, scheduled_moves) = tokio::try_join!(
            async {
                Ok::<_, CalendarEventError>(
                    self.postgres_calendar_event_source
                        .list_in_range(self.db_pool, user_id, event_range)
                        .await?,
                )
            },
            async {
                Ok::<_, CalendarEventError>(
                    self.postgres_event_series_source
                        .list_shown_in_range(self.db_pool, user_id, event_range)
                        .await?,
                )
            },
            async {
                Ok::<_, CalendarEventError>(
                    ListScheduledMoves::new(self.db_pool)
                        .execute(user_id, event_range.from, event_range.to)
                        .await?,
                )
            },
        )?;
        range_events.extend(
            shown_series
                .iter()
                .flat_map(|series| series.occurrences_in(event_range)),
        );
        range_events.sort_by_key(|range_event| (range_event.starts_at, range_event.id));
        Ok(CalendarRange {
            events: range_events,
            moves: scheduled_moves,
        })
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
    use crate::features::calendar_events::entities::calendar_event::{
        CalendarEventKind, NewCalendarEvent,
    };

    #[sqlx::test]
    async fn lists_single_events_and_series_occurrences_in_start_order(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let before_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-09T09:00:00Z"),
                ends_at: Some(instant("2026-11-09T09:30:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;
        let after_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                starts_at: instant("2026-11-12T09:00:00Z"),
                ends_at: Some(instant("2026-11-12T09:30:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;

        let listed_events = ListCalendarEvents::new(&db_pool)
            .execute(
                user_id,
                EventRange {
                    from: instant("2026-11-08T16:00:00Z"),
                    to: instant("2026-11-15T16:00:00Z"),
                },
            )
            .await
            .expect("the list should run")
            .events;

        let listed_ids: Vec<Uuid> = listed_events
            .iter()
            .map(|listed_event| listed_event.id)
            .collect();
        assert_eq!(listed_ids, vec![before_event_id, series_id, after_event_id]);
        assert_eq!(listed_events[1].starts_at, instant("2026-11-09T10:00:00Z"));
    }
}
