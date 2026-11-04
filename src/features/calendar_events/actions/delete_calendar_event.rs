use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::error::CalendarEventError;

pub struct DeleteCalendarEvent<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl<'a> DeleteCalendarEvent<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, event_id: Uuid) -> Result<(), CalendarEventError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let current_event = self
            .postgres_calendar_event_source
            .find_locked(&mut *delete_transaction, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        if current_event.is_running() {
            return Err(CalendarEventError::Invalid(String::from(
                "finish a running event before deleting it",
            )));
        }
        self.postgres_calendar_event_source
            .delete(&mut *delete_transaction, user_id, event_id)
            .await?;
        delete_transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, new_event, seed_event, seed_task_ids, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::{
        CalendarEventKind, NewCalendarEvent,
    };

    #[sqlx::test]
    async fn deletes_planned_and_finished_events(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let planned_event_id =
            seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;
        let finished_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                task_id: Some(task_id),
                ends_at: Some(instant("2026-11-05T09:40:00Z")),
                finished_at: Some(instant("2026-11-05T09:40:00Z")),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        let delete_calendar_event = DeleteCalendarEvent::new(&db_pool);

        for event_id in [planned_event_id, finished_event_id] {
            delete_calendar_event
                .execute(user_id, event_id)
                .await
                .expect("the event should be deleted");
            let deleted_again = delete_calendar_event.execute(user_id, event_id).await;
            assert!(matches!(deleted_again, Err(CalendarEventError::NotFound)));
        }
    }

    #[sqlx::test]
    async fn refuses_to_delete_a_running_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let running_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                task_id: Some(task_id),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;

        let delete_result = DeleteCalendarEvent::new(&db_pool)
            .execute(user_id, running_event_id)
            .await;

        assert!(matches!(delete_result, Err(CalendarEventError::Invalid(_))));
    }

    #[sqlx::test]
    async fn answers_not_found_for_another_person_s_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let their_event_id = seed_event(
            &db_pool,
            &new_event(other_user_id, CalendarEventKind::Planned),
        )
        .await;

        let delete_result = DeleteCalendarEvent::new(&db_pool)
            .execute(user_id, their_event_id)
            .await;

        assert!(matches!(delete_result, Err(CalendarEventError::NotFound)));
    }
}
