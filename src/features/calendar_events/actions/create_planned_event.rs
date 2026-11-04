use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventKind, EventWrite, NewCalendarEvent, PlannedEventInput,
};
use crate::features::calendar_events::error::CalendarEventError;

pub struct CreatePlannedEvent<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl<'a> CreatePlannedEvent<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        planned_event_input: &PlannedEventInput,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let new_event = NewCalendarEvent {
            id: Uuid::now_v7(),
            user_id,
            kind: CalendarEventKind::Planned,
            title: planned_event_input.title.clone(),
            category_id: planned_event_input.category_id,
            task_id: None,
            card_id: None,
            starts_at: planned_event_input.starts_at,
            ends_at: Some(planned_event_input.ends_at),
            finished_at: None,
            last_answered_at: None,
        };
        let mut create_transaction = self.db_pool.begin().await?;
        let event_write = self
            .postgres_calendar_event_source
            .insert(&mut *create_transaction, &new_event)
            .await?;
        if event_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        let created_event = self
            .postgres_calendar_event_source
            .find_locked(&mut *create_transaction, user_id, new_event.id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        create_transaction.commit().await?;
        Ok(created_event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, seed_category_id, seed_user_id,
    };

    fn gym_input(category_id: Option<Uuid>) -> PlannedEventInput {
        PlannedEventInput {
            title: String::from("Gym"),
            category_id,
            starts_at: instant("2026-11-05T09:00:00Z"),
            ends_at: instant("2026-11-05T10:00:00Z"),
        }
    }

    #[sqlx::test]
    async fn creates_a_planned_event_with_its_category(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;

        let created_event = CreatePlannedEvent::new(&db_pool)
            .execute(user_id, &gym_input(Some(category_id)))
            .await
            .expect("the event should be created");

        assert_eq!(created_event.kind, CalendarEventKind::Planned);
        assert_eq!(created_event.title, "Gym");
        assert_eq!(created_event.category_id, Some(category_id));
        assert_eq!(created_event.ends_at, Some(instant("2026-11-05T10:00:00Z")));
        assert_eq!(created_event.source, None);
    }

    #[sqlx::test]
    async fn refuses_another_person_s_category(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;

        let create_result = CreatePlannedEvent::new(&db_pool)
            .execute(user_id, &gym_input(Some(other_category_id)))
            .await;

        assert!(matches!(create_result, Err(CalendarEventError::NotFound)));
    }
}
