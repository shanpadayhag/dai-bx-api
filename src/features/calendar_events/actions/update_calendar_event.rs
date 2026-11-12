use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventChanges, ChangedCalendarEvent, EventRepeat, EventWrite,
};
use crate::features::calendar_events::entities::event_series::NewEventSeries;
use crate::features::calendar_events::error::CalendarEventError;

pub struct UpdateCalendarEvent<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl<'a> UpdateCalendarEvent<'a> {
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
        event_id: Uuid,
        event_changes: &CalendarEventChanges,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let current_event = self
            .postgres_calendar_event_source
            .find_locked(&mut *update_transaction, user_id, event_id)
            .await?
            .filter(|current_event| current_event.series.is_none())
            .ok_or(CalendarEventError::NotFound)?;
        let changed_event = current_event.apply_changes(event_changes, UtcInstant::now())?;
        let updated_event = match event_changes.new_repeat() {
            Some(event_repeat) => {
                self.turn_into_series(
                    &mut update_transaction,
                    user_id,
                    event_id,
                    &changed_event,
                    event_repeat,
                )
                .await?
            }
            None => {
                self.change_in_place(&mut update_transaction, user_id, event_id, &changed_event)
                    .await?
            }
        };
        update_transaction.commit().await?;
        Ok(updated_event)
    }

    async fn change_in_place(
        &self,
        update_transaction: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        event_id: Uuid,
        changed_event: &ChangedCalendarEvent,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let event_write = self
            .postgres_calendar_event_source
            .update(&mut **update_transaction, user_id, event_id, changed_event)
            .await?;
        if event_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        self.postgres_calendar_event_source
            .find_locked(&mut **update_transaction, user_id, event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)
    }

    async fn turn_into_series(
        &self,
        update_transaction: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        event_id: Uuid,
        changed_event: &ChangedCalendarEvent,
        event_repeat: &EventRepeat,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let series_zone = event_repeat.series_zone(changed_event.time_zone.as_deref())?;
        let new_series = NewEventSeries::starting_at(
            user_id,
            &changed_event.title,
            changed_event.category_id,
            changed_event.starts_at,
            changed_event.ends_at.unwrap_or(changed_event.starts_at),
            &series_zone,
            event_repeat.repeat_rule.clone(),
        )?;
        let series_write = self
            .postgres_event_series_source
            .insert(&mut **update_transaction, &new_series)
            .await?;
        if series_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        self.postgres_calendar_event_source
            .delete(&mut **update_transaction, user_id, event_id)
            .await?;
        new_series
            .into_series()
            .first_occurrence()
            .ok_or(CalendarEventError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        find_event, instant, new_event, seed_category_id, seed_event, seed_task_ids, seed_user_id,
    };
    use crate::features::calendar_events::data_sources::postgres::event_series_source::tests::{
        new_weekly_series, seed_series,
    };
    use crate::features::calendar_events::entities::calendar_event::{
        CalendarEventKind, NewCalendarEvent, parse_event_zone,
    };

    fn daily_repeat() -> EventRepeat {
        EventRepeat {
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
            )
            .expect("a valid rule"),
            input_zone: parse_event_zone("Asia/Manila", "inputZone").expect("a known zone"),
        }
    }

    #[sqlx::test]
    async fn turns_a_planned_event_into_a_series_with_its_changes(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let first_occurrence = UpdateCalendarEvent::new(&db_pool)
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    title: Some(String::from("Swim")),
                    repeat_change: Some(Some(daily_repeat())),
                    ..CalendarEventChanges::default()
                },
            )
            .await
            .expect("the event should become a series");

        assert_eq!(first_occurrence.title, "Swim");
        assert_eq!(first_occurrence.starts_at, instant("2026-11-05T09:00:00Z"));
        assert_eq!(
            first_occurrence.ends_at,
            Some(instant("2026-11-05T10:00:00Z"))
        );
        assert_eq!(first_occurrence.time_zone.as_deref(), Some("Asia/Manila"));
        assert!(first_occurrence.series.is_some());
        let old_event = PostgresCalendarEventSource
            .find_locked(&db_pool, user_id, event_id)
            .await
            .expect("the read should run");
        assert_eq!(old_event, None);
    }

    #[sqlx::test]
    async fn keeps_the_event_when_its_repeat_never_happens(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let update_result = UpdateCalendarEvent::new(&db_pool)
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    repeat_change: Some(Some(EventRepeat {
                        repeat_rule: serde_json::from_str(
                            r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2026-11-01"}}"#,
                        )
                        .expect("a valid rule"),
                        ..daily_repeat()
                    })),
                    ..CalendarEventChanges::default()
                },
            )
            .await;

        assert!(matches!(update_result, Err(CalendarEventError::Invalid(_))));
        assert_eq!(
            find_event(&db_pool, user_id, event_id).await.title,
            "Focus block"
        );
    }

    #[sqlx::test]
    async fn answers_not_found_for_an_override_row(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let override_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                series_id: Some(series_id),
                original_start: Some(instant("2026-11-16T10:00:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;

        let update_result = UpdateCalendarEvent::new(&db_pool)
            .execute(user_id, override_id, &CalendarEventChanges::default())
            .await;

        assert!(matches!(update_result, Err(CalendarEventError::NotFound)));
    }

    #[sqlx::test]
    async fn changes_a_planned_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let updated_event = UpdateCalendarEvent::new(&db_pool)
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    title: Some(String::from("Swim")),
                    ends_at: Some(instant("2026-11-06T10:00:00Z")),
                    ..CalendarEventChanges::default()
                },
            )
            .await
            .expect("the change should be saved");

        assert_eq!(updated_event.title, "Swim");
        assert_eq!(updated_event.ends_at, Some(instant("2026-11-06T10:00:00Z")));
    }

    #[sqlx::test]
    async fn moves_a_finished_task_event_s_finish_along_with_a_later_end(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                task_id: Some(task_id),
                ends_at: Some(instant("2025-11-05T09:40:00Z")),
                finished_at: Some(instant("2025-11-05T10:00:00Z")),
                last_answered_at: Some(instant("2025-11-05T09:40:00Z")),
                starts_at: instant("2025-11-05T09:00:00Z"),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;

        let updated_event = UpdateCalendarEvent::new(&db_pool)
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    ends_at: Some(instant("2025-11-05T10:15:00Z")),
                    ..CalendarEventChanges::default()
                },
            )
            .await
            .expect("the later end should be saved");

        assert_eq!(updated_event.ends_at, Some(instant("2025-11-05T10:15:00Z")));
        assert_eq!(
            updated_event.finished_at,
            Some(instant("2025-11-05T10:15:00Z"))
        );
        assert_eq!(updated_event.title, "Focus block");
    }

    #[sqlx::test]
    async fn changes_only_the_category_of_a_running_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                task_id: Some(task_id),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        let update_calendar_event = UpdateCalendarEvent::new(&db_pool);

        let recategorized = update_calendar_event
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    category_change: Some(Some(category_id)),
                    ..CalendarEventChanges::default()
                },
            )
            .await
            .expect("the category should change");
        let retimed = update_calendar_event
            .execute(
                user_id,
                event_id,
                &CalendarEventChanges {
                    ends_at: Some(instant("2026-11-05T09:30:00Z")),
                    ..CalendarEventChanges::default()
                },
            )
            .await;

        assert_eq!(recategorized.category_id, Some(category_id));
        assert!(recategorized.is_running());
        assert!(matches!(retimed, Err(CalendarEventError::Invalid(_))));
        assert!(find_event(&db_pool, user_id, event_id).await.is_running());
    }

    #[sqlx::test]
    async fn answers_not_found_for_another_person_s_event_or_category(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let their_event_id = seed_event(
            &db_pool,
            &new_event(other_user_id, CalendarEventKind::Planned),
        )
        .await;
        let my_event_id =
            seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;
        let update_calendar_event = UpdateCalendarEvent::new(&db_pool);

        let their_event_result = update_calendar_event
            .execute(user_id, their_event_id, &CalendarEventChanges::default())
            .await;
        let their_category_result = update_calendar_event
            .execute(
                user_id,
                my_event_id,
                &CalendarEventChanges {
                    category_change: Some(Some(other_category_id)),
                    ..CalendarEventChanges::default()
                },
            )
            .await;

        assert!(matches!(
            their_event_result,
            Err(CalendarEventError::NotFound)
        ));
        assert!(matches!(
            their_category_result,
            Err(CalendarEventError::NotFound)
        ));
    }
}
