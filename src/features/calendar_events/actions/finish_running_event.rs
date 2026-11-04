use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::CalendarEvent;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct FinishRunningEvent {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl FinishRunningEvent {
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
        requested_ends_at: UtcInstant,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let running_event = self
            .postgres_calendar_event_source
            .find_locked(&mut *connection, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        let finished_event = running_event.finish(requested_ends_at, UtcInstant::now())?;
        self.postgres_calendar_event_source
            .update(&mut *connection, user_id, event_id, &finished_event)
            .await?;
        self.postgres_calendar_event_source
            .find_locked(&mut *connection, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, new_event, seed_event, seed_task_ids, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::{
        CalendarEventKind, NewCalendarEvent,
    };

    #[sqlx::test]
    async fn finishes_a_running_event_once(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let running_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                task_id: Some(task_id),
                starts_at: instant("2025-11-05T09:00:00Z"),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        let mut connection = db_pool.acquire().await.expect("a connection");
        let finish_running_event = FinishRunningEvent::new();

        let finished_event = finish_running_event
            .execute(
                &mut connection,
                user_id,
                running_event_id,
                instant("2025-11-05T09:40:00Z"),
            )
            .await
            .expect("the event should finish");
        let second_finish = finish_running_event
            .execute(
                &mut connection,
                user_id,
                running_event_id,
                instant("2025-11-05T09:50:00Z"),
            )
            .await;

        assert_eq!(
            finished_event.ends_at,
            Some(instant("2025-11-05T09:40:00Z"))
        );
        let finished_at = finished_event
            .finished_at
            .expect("the finish time should be stamped");
        assert!(finished_at > instant("2026-01-01T00:00:00Z"));
        assert!(matches!(second_finish, Err(CalendarEventError::Invalid(_))));
    }

    #[sqlx::test]
    async fn answers_not_found_for_another_person_s_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let their_event_id = seed_event(
            &db_pool,
            &new_event(other_user_id, CalendarEventKind::Tracked),
        )
        .await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        let finish_result = FinishRunningEvent::new()
            .execute(
                &mut connection,
                user_id,
                their_event_id,
                instant("2026-11-05T09:40:00Z"),
            )
            .await;

        assert!(matches!(finish_result, Err(CalendarEventError::NotFound)));
    }
}
