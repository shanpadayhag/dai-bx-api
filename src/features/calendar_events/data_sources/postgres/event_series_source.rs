use jiff::civil::DateTime;
use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::RepeatRule;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::calendar_event::{EventRange, EventWrite};
use crate::features::calendar_events::entities::event_series::{EventSeries, NewEventSeries};

#[derive(Default)]
pub struct PostgresEventSeriesSource;

struct EventSeriesRow {
    id: Uuid,
    title: String,
    category_id: Option<Uuid>,
    time_zone: String,
    local_start_text: String,
    duration_minutes: i32,
    repeat_rule: Json<RepeatRule>,
    until_instant_millis: Option<i64>,
    cancelled_start_millis: Vec<i64>,
}

impl TryFrom<EventSeriesRow> for EventSeries {
    type Error = sqlx::Error;

    fn try_from(series_row: EventSeriesRow) -> Result<Self, Self::Error> {
        let local_start = series_row
            .local_start_text
            .parse::<DateTime>()
            .map_err(|parse_error| sqlx::Error::Decode(Box::new(parse_error)))?;
        Ok(Self {
            id: series_row.id,
            title: series_row.title,
            category_id: series_row.category_id,
            time_zone: series_row.time_zone,
            local_start,
            duration_minutes: series_row.duration_minutes,
            repeat_rule: series_row.repeat_rule.0,
            until_instant: series_row
                .until_instant_millis
                .map(UtcInstant::from_unix_millis),
            cancelled_starts: series_row
                .cancelled_start_millis
                .into_iter()
                .map(UtcInstant::from_unix_millis)
                .collect(),
        })
    }
}

fn millis_of(instants: &[UtcInstant]) -> Vec<i64> {
    instants
        .iter()
        .map(|instant| instant.unix_millis())
        .collect()
}

impl PostgresEventSeriesSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_series: &NewEventSeries,
    ) -> Result<EventWrite, sqlx::Error> {
        let insert_result = sqlx::query!(
            "INSERT INTO event_series
                 (id, user_id, title, category_id, time_zone, local_start,
                  duration_minutes, repeat_rule, until_instant)
             VALUES ($1, $2, $3, $4, $5, $6::text::timestamp, $7, $8,
                     'epoch'::timestamptz + $9::bigint * interval '1 millisecond')",
            new_series.id,
            new_series.user_id,
            new_series.title,
            new_series.category_id,
            new_series.time_zone,
            new_series.local_start.to_string(),
            new_series.duration_minutes,
            Json(&new_series.repeat_rule) as _,
            new_series.until_instant.map(UtcInstant::unix_millis),
        )
        .execute(executor)
        .await;
        match insert_result {
            Ok(_) => Ok(EventWrite::Written),
            Err(sqlx::Error::Database(database_error))
                if database_error.is_foreign_key_violation() =>
            {
                Ok(EventWrite::UnknownLink)
            }
            Err(other_error) => Err(other_error),
        }
    }

    pub async fn list_shown_in_range(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_range: EventRange,
    ) -> Result<Vec<EventSeries>, sqlx::Error> {
        let series_rows = sqlx::query_as!(
            EventSeriesRow,
            r#"SELECT event_series.id,
                      event_series.title,
                      event_series.category_id,
                      event_series.time_zone,
                      to_char(event_series.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      event_series.duration_minutes,
                      event_series.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      (extract(epoch FROM event_series.until_instant) * 1000)::bigint AS until_instant_millis,
                      coalesce(
                          (SELECT array_agg((extract(epoch FROM event_exceptions.original_start) * 1000)::bigint)
                           FROM event_exceptions
                           WHERE event_exceptions.series_id = event_series.id
                             AND event_exceptions.original_start
                                 < 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
                             AND event_exceptions.original_start
                                 > 'epoch'::timestamptz + $2::bigint * interval '1 millisecond'
                                   - event_series.duration_minutes * interval '1 minute'),
                          '{}'
                      ) AS "cancelled_start_millis!"
               FROM event_series
               WHERE event_series.user_id = $1
                 AND NOT event_series.hidden
                 AND (event_series.until_instant IS NULL
                      OR event_series.until_instant
                         > 'epoch'::timestamptz + $2::bigint * interval '1 millisecond'
                           - event_series.duration_minutes * interval '1 minute')
               ORDER BY event_series.id"#,
            user_id,
            event_range.from.unix_millis(),
            event_range.to.unix_millis(),
        )
        .fetch_all(executor)
        .await?;
        series_rows.into_iter().map(EventSeries::try_from).collect()
    }

    pub async fn find_locked(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        series_id: Uuid,
    ) -> Result<Option<EventSeries>, sqlx::Error> {
        let series_row = sqlx::query_as!(
            EventSeriesRow,
            r#"SELECT event_series.id,
                      event_series.title,
                      event_series.category_id,
                      event_series.time_zone,
                      to_char(event_series.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      event_series.duration_minutes,
                      event_series.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      (extract(epoch FROM event_series.until_instant) * 1000)::bigint AS until_instant_millis,
                      coalesce(
                          (SELECT array_agg((extract(epoch FROM event_exceptions.original_start) * 1000)::bigint
                                            ORDER BY event_exceptions.original_start)
                           FROM event_exceptions
                           WHERE event_exceptions.series_id = event_series.id),
                          '{}'
                      ) AS "cancelled_start_millis!"
               FROM event_series
               WHERE event_series.id = $2
                 AND event_series.user_id = $1
                 AND NOT event_series.hidden
               FOR UPDATE OF event_series"#,
            user_id,
            series_id,
        )
        .fetch_optional(executor)
        .await?;
        series_row.map(EventSeries::try_from).transpose()
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        changed_series: &EventSeries,
    ) -> Result<EventWrite, sqlx::Error> {
        let update_result = sqlx::query!(
            "UPDATE event_series
             SET title = $3,
                 category_id = $4,
                 time_zone = $5,
                 local_start = $6::text::timestamp,
                 duration_minutes = $7,
                 repeat_rule = $8,
                 until_instant = 'epoch'::timestamptz + $9::bigint * interval '1 millisecond',
                 updated_at = now()
             WHERE id = $2 AND user_id = $1",
            user_id,
            changed_series.id,
            changed_series.title,
            changed_series.category_id,
            changed_series.time_zone,
            changed_series.local_start.to_string(),
            changed_series.duration_minutes,
            Json(&changed_series.repeat_rule) as _,
            changed_series.until_instant.map(UtcInstant::unix_millis),
        )
        .execute(executor)
        .await;
        match update_result {
            Ok(_) => Ok(EventWrite::Written),
            Err(sqlx::Error::Database(database_error))
                if database_error.is_foreign_key_violation() =>
            {
                Ok(EventWrite::UnknownLink)
            }
            Err(other_error) => Err(other_error),
        }
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        series_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_result = sqlx::query!(
            "DELETE FROM event_series WHERE id = $2 AND user_id = $1",
            user_id,
            series_id,
        )
        .execute(executor)
        .await?;
        Ok(delete_result.rows_affected() == 1)
    }

    pub async fn insert_exception(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        series_id: Uuid,
        original_start: UtcInstant,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO event_exceptions (series_id, user_id, original_start)
             VALUES ($2, $1, 'epoch'::timestamptz + $3::bigint * interval '1 millisecond')
             ON CONFLICT DO NOTHING",
            user_id,
            series_id,
            original_start.unix_millis(),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete_exceptions(
        &self,
        executor: impl PgExecutor<'_>,
        series_id: Uuid,
        original_starts: &[UtcInstant],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM event_exceptions
             WHERE series_id = $1
               AND original_start = ANY(
                   SELECT 'epoch'::timestamptz + start_millis * interval '1 millisecond'
                   FROM unnest($2::bigint[]) AS start_millis)",
            series_id,
            &millis_of(original_starts),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn move_exceptions(
        &self,
        executor: impl PgExecutor<'_>,
        from_series_id: Uuid,
        to_series_id: Uuid,
        original_starts: &[UtcInstant],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE event_exceptions
             SET series_id = $2
             WHERE series_id = $1
               AND original_start = ANY(
                   SELECT 'epoch'::timestamptz + start_millis * interval '1 millisecond'
                   FROM unnest($3::bigint[]) AS start_millis)",
            from_series_id,
            to_series_id,
            &millis_of(original_starts),
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        find_event, instant, new_event, seed_category_id, seed_event, seed_user_id,
    };
    use crate::features::calendar_events::entities::calendar_event::{
        CalendarEventKind, NewCalendarEvent, SeriesOccurrence,
    };

    pub fn new_weekly_series(user_id: Uuid) -> NewEventSeries {
        NewEventSeries {
            id: Uuid::now_v7(),
            user_id,
            title: String::from("Gym"),
            category_id: None,
            time_zone: String::from("Asia/Manila"),
            local_start: "2026-11-09T18:00:00".parse().expect("a local time"),
            duration_minutes: 60,
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
            )
            .expect("a valid rule"),
            until_instant: None,
        }
    }

    pub async fn seed_series(db_pool: &PgPool, new_series: &NewEventSeries) -> Uuid {
        let series_write = PostgresEventSeriesSource
            .insert(db_pool, new_series)
            .await
            .expect("the insert should run");
        assert_eq!(series_write, EventWrite::Written);
        new_series.id
    }

    fn manila_week() -> EventRange {
        EventRange {
            from: instant("2026-11-08T16:00:00Z"),
            to: instant("2026-11-15T16:00:00Z"),
        }
    }

    #[sqlx::test]
    async fn reads_back_every_field_of_a_series(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let new_series = NewEventSeries {
            category_id: Some(category_id),
            local_start: "2026-11-09T18:30:15".parse().expect("a local time"),
            ..new_weekly_series(user_id)
        };
        seed_series(&db_pool, &new_series).await;

        let listed_series = PostgresEventSeriesSource
            .list_shown_in_range(&db_pool, user_id, manila_week())
            .await
            .expect("the list should run");

        assert_eq!(listed_series, vec![new_series.into_series()]);
    }

    #[sqlx::test]
    async fn lists_only_the_owner_s_series(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        seed_series(&db_pool, &new_weekly_series(other_user_id)).await;

        let listed_series = PostgresEventSeriesSource
            .list_shown_in_range(&db_pool, user_id, manila_week())
            .await
            .expect("the list should run");

        let listed_ids: Vec<Uuid> = listed_series.iter().map(|series| series.id).collect();
        assert_eq!(listed_ids, vec![series_id]);
    }

    async fn find_series(db_pool: &PgPool, user_id: Uuid, series_id: Uuid) -> Option<EventSeries> {
        PostgresEventSeriesSource
            .find_locked(db_pool, user_id, series_id)
            .await
            .expect("the read should run")
    }

    #[sqlx::test]
    async fn finds_only_the_owner_s_series_with_every_cancelled_start(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let new_series = NewEventSeries {
            until_instant: Some(instant("2027-01-04T10:00:00Z")),
            ..new_weekly_series(user_id)
        };
        let series_id = seed_series(&db_pool, &new_series).await;
        for cancelled_start in ["2026-12-21T10:00:00Z", "2026-11-16T10:00:00Z"] {
            PostgresEventSeriesSource
                .insert_exception(&db_pool, user_id, series_id, instant(cancelled_start))
                .await
                .expect("the insert should run");
        }

        let found_series = find_series(&db_pool, user_id, series_id).await;

        assert_eq!(
            found_series,
            Some(EventSeries {
                cancelled_starts: vec![
                    instant("2026-11-16T10:00:00Z"),
                    instant("2026-12-21T10:00:00Z"),
                ],
                ..new_series.into_series()
            })
        );
        assert_eq!(find_series(&db_pool, other_user_id, series_id).await, None);
    }

    #[sqlx::test]
    async fn changes_every_field_of_a_series_and_deletes_it(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let changed_series = EventSeries {
            id: series_id,
            title: String::from("Swim"),
            category_id: Some(category_id),
            time_zone: String::from("Europe/London"),
            local_start: "2026-11-10T07:15:00".parse().expect("a local time"),
            duration_minutes: 30,
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"daily","interval":2,"ends":{"after":5}}"#,
            )
            .expect("a valid rule"),
            until_instant: Some(instant("2026-11-20T00:00:00Z")),
            ..new_weekly_series(user_id).into_series()
        };

        let series_write = PostgresEventSeriesSource
            .update(&db_pool, user_id, &changed_series)
            .await
            .expect("the update should run");
        let found_series = find_series(&db_pool, user_id, series_id).await;
        let is_deleted_by_other = PostgresEventSeriesSource
            .delete(&db_pool, Uuid::now_v7(), series_id)
            .await
            .expect("the delete should run");
        let is_deleted = PostgresEventSeriesSource
            .delete(&db_pool, user_id, series_id)
            .await
            .expect("the delete should run");

        assert_eq!(series_write, EventWrite::Written);
        assert_eq!(found_series, Some(changed_series));
        assert!(!is_deleted_by_other);
        assert!(is_deleted);
        assert_eq!(find_series(&db_pool, user_id, series_id).await, None);
    }

    #[sqlx::test]
    async fn moves_and_deletes_only_the_named_cancelled_starts(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let following_series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let cancelled_starts = [
            instant("2026-11-16T10:00:00Z"),
            instant("2026-11-23T10:00:00Z"),
            instant("2026-11-30T10:00:00Z"),
        ];
        for cancelled_start in cancelled_starts {
            PostgresEventSeriesSource
                .insert_exception(&db_pool, user_id, series_id, cancelled_start)
                .await
                .expect("the insert should run");
        }
        PostgresEventSeriesSource
            .insert_exception(&db_pool, user_id, series_id, cancelled_starts[0])
            .await
            .expect("a repeated insert should run");

        PostgresEventSeriesSource
            .move_exceptions(
                &db_pool,
                series_id,
                following_series_id,
                &cancelled_starts[2..],
            )
            .await
            .expect("the move should run");
        PostgresEventSeriesSource
            .delete_exceptions(&db_pool, series_id, &cancelled_starts[1..2])
            .await
            .expect("the delete should run");

        let cancelled_of = |found_series: Option<EventSeries>| {
            found_series
                .expect("the series should exist")
                .cancelled_starts
        };
        assert_eq!(
            cancelled_of(find_series(&db_pool, user_id, series_id).await),
            cancelled_starts[..1]
        );
        assert_eq!(
            cancelled_of(find_series(&db_pool, user_id, following_series_id).await),
            cancelled_starts[2..]
        );
    }

    #[sqlx::test]
    async fn reports_another_person_s_category_as_an_unknown_link(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;

        let series_write = PostgresEventSeriesSource
            .insert(
                &db_pool,
                &NewEventSeries {
                    category_id: Some(other_category_id),
                    ..new_weekly_series(user_id)
                },
            )
            .await
            .expect("the insert should run");

        assert_eq!(series_write, EventWrite::UnknownLink);
    }

    #[sqlx::test]
    async fn reads_the_series_an_override_replaces_an_occurrence_of(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let new_series = new_weekly_series(user_id);
        let series_id = seed_series(&db_pool, &new_series).await;
        let override_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                time_zone: Some(String::from("Asia/Manila")),
                series_id: Some(series_id),
                original_start: Some(instant("2026-11-16T10:00:00Z")),
                starts_at: instant("2026-11-16T11:00:00Z"),
                ends_at: Some(instant("2026-11-16T12:00:00Z")),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await;

        let override_event = find_event(&db_pool, user_id, override_id).await;
        let week_events = PostgresCalendarEventSource
            .list_in_range(
                &db_pool,
                user_id,
                EventRange {
                    from: instant("2026-11-15T16:00:00Z"),
                    to: instant("2026-11-22T16:00:00Z"),
                },
            )
            .await
            .expect("the list should run");

        let expected_series = Some(SeriesOccurrence {
            series_id,
            original_start: instant("2026-11-16T10:00:00Z"),
            repeat_rule: new_series.repeat_rule,
        });
        assert_eq!(override_event.series, expected_series);
        assert_eq!(override_event.time_zone.as_deref(), Some("Asia/Manila"));
        assert_eq!(week_events.len(), 1);
        assert_eq!(week_events[0].series, expected_series);
    }

    async fn seed_override(
        db_pool: &PgPool,
        user_id: Uuid,
        series_id: Uuid,
        original_start: &str,
    ) -> Uuid {
        seed_event(
            db_pool,
            &NewCalendarEvent {
                time_zone: Some(String::from("Asia/Manila")),
                series_id: Some(series_id),
                original_start: Some(instant(original_start)),
                starts_at: instant(original_start),
                ends_at: Some(UtcInstant::from_unix_millis(
                    instant(original_start).unix_millis() + 3_600_000,
                )),
                ..new_event(user_id, CalendarEventKind::Planned)
            },
        )
        .await
    }

    #[sqlx::test]
    async fn finds_an_override_by_its_series_and_original_start(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let override_id = seed_override(&db_pool, user_id, series_id, "2026-11-16T10:00:00Z").await;

        let find_override = |wanted_user_id: Uuid, original_start: &'static str| {
            let db_pool = db_pool.clone();
            async move {
                PostgresCalendarEventSource
                    .find_override_locked(
                        &db_pool,
                        wanted_user_id,
                        series_id,
                        instant(original_start),
                    )
                    .await
                    .expect("the read should run")
            }
        };

        assert_eq!(
            find_override(user_id, "2026-11-16T10:00:00Z").await,
            Some(find_event(&db_pool, user_id, override_id).await)
        );
        assert_eq!(find_override(user_id, "2026-11-23T10:00:00Z").await, None);
        assert_eq!(
            find_override(other_user_id, "2026-11-16T10:00:00Z").await,
            None
        );
    }

    #[sqlx::test]
    async fn lists_moves_and_deletes_a_series_overrides(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let following_series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        let original_starts = [
            instant("2026-11-16T10:00:00Z"),
            instant("2026-11-23T10:00:00Z"),
            instant("2026-11-30T10:00:00Z"),
        ];
        for original_start in [
            "2026-11-30T10:00:00Z",
            "2026-11-16T10:00:00Z",
            "2026-11-23T10:00:00Z",
        ] {
            seed_override(&db_pool, user_id, series_id, original_start).await;
        }
        let listed_starts = PostgresCalendarEventSource
            .list_override_starts(&db_pool, series_id)
            .await
            .expect("the list should run");

        PostgresCalendarEventSource
            .move_overrides(
                &db_pool,
                series_id,
                following_series_id,
                &original_starts[2..],
            )
            .await
            .expect("the move should run");
        PostgresCalendarEventSource
            .delete_overrides(&db_pool, series_id, &original_starts[1..2])
            .await
            .expect("the delete should run");

        let starts_of = |series_id: Uuid| {
            let db_pool = db_pool.clone();
            async move {
                PostgresCalendarEventSource
                    .list_override_starts(&db_pool, series_id)
                    .await
                    .expect("the list should run")
            }
        };
        assert_eq!(listed_starts, original_starts);
        assert_eq!(starts_of(series_id).await, original_starts[..1]);
        assert_eq!(starts_of(following_series_id).await, original_starts[2..]);
    }
}
