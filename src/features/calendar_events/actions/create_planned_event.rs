use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventKind, EventRepeat, EventWrite, NewCalendarEvent, PlannedEventInput,
};
use crate::features::calendar_events::entities::event_series::NewEventSeries;
use crate::features::calendar_events::error::CalendarEventError;

pub struct CreatePlannedEvent<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl<'a> CreatePlannedEvent<'a> {
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
        planned_event_input: &PlannedEventInput,
    ) -> Result<CalendarEvent, CalendarEventError> {
        match &planned_event_input.repeat {
            Some(event_repeat) => {
                self.create_series(user_id, planned_event_input, event_repeat)
                    .await
            }
            None => self.create_single(user_id, planned_event_input).await,
        }
    }

    async fn create_series(
        &self,
        user_id: Uuid,
        planned_event_input: &PlannedEventInput,
        event_repeat: &EventRepeat,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let series_zone = event_repeat.series_zone(planned_event_input.time_zone.as_deref())?;
        let new_series = NewEventSeries::starting_at(
            user_id,
            &planned_event_input.title,
            planned_event_input.category_id,
            planned_event_input.starts_at,
            planned_event_input.ends_at,
            &series_zone,
            event_repeat.repeat_rule.clone(),
        )?;
        let series_write = self
            .postgres_event_series_source
            .insert(self.db_pool, &new_series)
            .await?;
        if series_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        new_series
            .into_series()
            .first_occurrence()
            .ok_or(CalendarEventError::NotFound)
    }

    async fn create_single(
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
            time_zone: planned_event_input.time_zone.clone(),
            series_id: None,
            original_start: None,
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
    use crate::core::recurrence::RepeatRule;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, seed_category_id, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::parse_event_zone;

    fn gym_input(category_id: Option<Uuid>) -> PlannedEventInput {
        PlannedEventInput {
            title: String::from("Gym"),
            category_id,
            starts_at: instant("2026-11-05T09:00:00Z"),
            ends_at: instant("2026-11-05T10:00:00Z"),
            time_zone: None,
            repeat: None,
        }
    }

    fn weekly_on_thursday() -> RepeatRule {
        serde_json::from_str(
            r#"{"frequency":"weekly","interval":1,"weekdays":["thu"],"ends":{"never":true}}"#,
        )
        .expect("a valid rule")
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
        assert_eq!(created_event.series, None);
    }

    #[sqlx::test]
    async fn creates_a_series_in_the_event_zone_or_else_the_input_zone(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let manila = parse_event_zone("Asia/Manila", "inputZone").expect("a known zone");
        let create_planned_event = CreatePlannedEvent::new(&db_pool);

        let input_zone_occurrence = create_planned_event
            .execute(
                user_id,
                &PlannedEventInput {
                    repeat: Some(EventRepeat {
                        repeat_rule: weekly_on_thursday(),
                        input_zone: manila.clone(),
                    }),
                    ..gym_input(None)
                },
            )
            .await
            .expect("the series should be created");
        let event_zone_occurrence = create_planned_event
            .execute(
                user_id,
                &PlannedEventInput {
                    time_zone: Some(String::from("Europe/London")),
                    repeat: Some(EventRepeat {
                        repeat_rule: weekly_on_thursday(),
                        input_zone: manila,
                    }),
                    ..gym_input(None)
                },
            )
            .await
            .expect("the series should be created");

        assert_eq!(
            input_zone_occurrence.time_zone.as_deref(),
            Some("Asia/Manila")
        );
        assert_eq!(
            event_zone_occurrence.time_zone.as_deref(),
            Some("Europe/London")
        );
        assert_eq!(
            input_zone_occurrence.starts_at,
            instant("2026-11-05T09:00:00Z")
        );
        assert_eq!(
            input_zone_occurrence
                .series
                .map(|series| series.repeat_rule),
            Some(weekly_on_thursday())
        );
    }

    #[sqlx::test]
    async fn refuses_another_person_s_category(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let create_planned_event = CreatePlannedEvent::new(&db_pool);

        let single_result = create_planned_event
            .execute(user_id, &gym_input(Some(other_category_id)))
            .await;
        let series_result = create_planned_event
            .execute(
                user_id,
                &PlannedEventInput {
                    repeat: Some(EventRepeat {
                        repeat_rule: weekly_on_thursday(),
                        input_zone: parse_event_zone("UTC", "inputZone").expect("a known zone"),
                    }),
                    ..gym_input(Some(other_category_id))
                },
            )
            .await;

        assert!(matches!(single_result, Err(CalendarEventError::NotFound)));
        assert!(matches!(series_result, Err(CalendarEventError::NotFound)));
    }
}
