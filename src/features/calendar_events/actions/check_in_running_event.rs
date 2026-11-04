use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::RunningItem;
use crate::features::calendar_events::error::CalendarEventError;

pub struct CheckInRunningEvent<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl<'a> CheckInRunningEvent<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<RunningItem, CalendarEventError> {
        let mut check_in_transaction = self.db_pool.begin().await?;
        let current_event = self
            .postgres_calendar_event_source
            .find_locked(&mut *check_in_transaction, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        if !current_event.is_running() {
            return Err(CalendarEventError::Invalid(String::from(
                "only a running event takes a check-in",
            )));
        }
        self.postgres_calendar_event_source
            .record_check_in(&mut *check_in_transaction, user_id, event_id)
            .await?;
        let checked_in_item = self
            .postgres_calendar_event_source
            .find_running_item(&mut *check_in_transaction, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        check_in_transaction.commit().await?;
        Ok(checked_in_item)
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
    async fn records_the_answer_on_a_running_event(db_pool: PgPool) {
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

        let checked_in_item = CheckInRunningEvent::new(&db_pool)
            .execute(user_id, running_event_id)
            .await
            .expect("the check-in should be recorded");

        assert!(checked_in_item.event.last_answered_at.is_some());
        assert!(checked_in_item.event.is_running());
    }

    #[sqlx::test]
    async fn refuses_a_finished_or_planned_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let finished_event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                ends_at: Some(instant("2026-11-05T09:30:00Z")),
                finished_at: Some(instant("2026-11-05T09:30:00Z")),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        let planned_event_id =
            seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        for event_id in [finished_event_id, planned_event_id] {
            let check_in_result = CheckInRunningEvent::new(&db_pool)
                .execute(user_id, event_id)
                .await;
            assert!(matches!(
                check_in_result,
                Err(CalendarEventError::Invalid(_))
            ));
        }
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

        let check_in_result = CheckInRunningEvent::new(&db_pool)
            .execute(user_id, their_event_id)
            .await;

        assert!(matches!(check_in_result, Err(CalendarEventError::NotFound)));
    }
}
