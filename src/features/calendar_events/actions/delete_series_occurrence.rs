use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::event_series::{EventSeries, OccurrenceScope};
use crate::features::calendar_events::error::CalendarEventError;

pub struct DeleteSeriesOccurrence<'a> {
    db_pool: &'a PgPool,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

impl<'a> DeleteSeriesOccurrence<'a> {
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
        series_id: Uuid,
        original_start: UtcInstant,
        occurrence_scope: OccurrenceScope,
    ) -> Result<(), CalendarEventError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let series = self
            .postgres_event_series_source
            .find_locked(&mut *delete_transaction, user_id, series_id)
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        let override_event = self
            .postgres_calendar_event_source
            .find_override_locked(&mut *delete_transaction, user_id, series_id, original_start)
            .await?;
        if override_event.is_none() && !series.holds_open_occurrence(original_start) {
            return Err(CalendarEventError::NotFound);
        }
        match series.widened_scope(occurrence_scope, original_start) {
            OccurrenceScope::This => {
                if let Some(override_event) = override_event {
                    self.postgres_calendar_event_source
                        .delete(&mut *delete_transaction, user_id, override_event.id)
                        .await?;
                }
                self.postgres_event_series_source
                    .insert_exception(&mut *delete_transaction, user_id, series_id, original_start)
                    .await?;
            }
            OccurrenceScope::Following => {
                self.cut_off_from(&mut delete_transaction, user_id, &series, original_start)
                    .await?;
            }
            OccurrenceScope::All => {
                self.postgres_event_series_source
                    .delete(&mut *delete_transaction, user_id, series_id)
                    .await?;
            }
        }
        delete_transaction.commit().await?;
        Ok(())
    }

    async fn cut_off_from(
        &self,
        delete_transaction: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        series: &EventSeries,
        original_start: UtcInstant,
    ) -> Result<(), CalendarEventError> {
        self.postgres_event_series_source
            .update(
                &mut **delete_transaction,
                user_id,
                &EventSeries {
                    until_instant: Some(original_start),
                    ..series.clone()
                },
            )
            .await?;
        let later_override_starts: Vec<UtcInstant> = self
            .postgres_calendar_event_source
            .list_override_starts(&mut **delete_transaction, series.id)
            .await?
            .into_iter()
            .filter(|override_start| *override_start >= original_start)
            .collect();
        let later_cancelled_starts: Vec<UtcInstant> = series
            .cancelled_starts
            .iter()
            .copied()
            .filter(|cancelled_start| *cancelled_start >= original_start)
            .collect();
        self.postgres_calendar_event_source
            .delete_overrides(&mut **delete_transaction, series.id, &later_override_starts)
            .await?;
        self.postgres_event_series_source
            .delete_exceptions(
                &mut **delete_transaction,
                series.id,
                &later_cancelled_starts,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, seed_user_id,
    };
    use crate::features::calendar_events::data_sources::postgres::event_series_source::tests::{
        new_weekly_series, seed_series,
    };
    use crate::features::calendar_events::entities::calendar_event::EventRange;

    const NOV_9: &str = "2026-11-09T10:00:00Z";
    const NOV_16: &str = "2026-11-16T10:00:00Z";
    const NOV_23: &str = "2026-11-23T10:00:00Z";

    async fn delete(
        db_pool: &PgPool,
        user_id: Uuid,
        series_id: Uuid,
        original_start: &str,
        occurrence_scope: OccurrenceScope,
    ) -> Result<(), CalendarEventError> {
        DeleteSeriesOccurrence::new(db_pool)
            .execute(
                user_id,
                series_id,
                instant(original_start),
                occurrence_scope,
            )
            .await
    }

    async fn find_series(db_pool: &PgPool, user_id: Uuid, series_id: Uuid) -> Option<EventSeries> {
        PostgresEventSeriesSource
            .find_locked(db_pool, user_id, series_id)
            .await
            .expect("the read should run")
    }

    #[sqlx::test]
    async fn cancels_one_occurrence_and_refuses_it_a_second_time(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;

        delete(&db_pool, user_id, series_id, NOV_16, OccurrenceScope::This)
            .await
            .expect("the delete should run");
        let deleted_again =
            delete(&db_pool, user_id, series_id, NOV_16, OccurrenceScope::This).await;

        assert!(matches!(deleted_again, Err(CalendarEventError::NotFound)));
        assert_eq!(
            find_series(&db_pool, user_id, series_id)
                .await
                .map(|series| series.cancelled_starts),
            Some(vec![instant(NOV_16)])
        );
    }

    #[sqlx::test]
    async fn cuts_the_series_off_at_the_occurrence_and_forgets_later_cancellations(
        db_pool: PgPool,
    ) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        delete(
            &db_pool,
            user_id,
            series_id,
            "2026-11-30T10:00:00Z",
            OccurrenceScope::This,
        )
        .await
        .expect("the delete should run");

        delete(
            &db_pool,
            user_id,
            series_id,
            NOV_23,
            OccurrenceScope::Following,
        )
        .await
        .expect("the delete should run");

        let cut_series = find_series(&db_pool, user_id, series_id)
            .await
            .expect("the series should remain");
        assert_eq!(cut_series.until_instant, Some(instant(NOV_23)));
        assert_eq!(cut_series.cancelled_starts, Vec::new());
        let shown_starts: Vec<UtcInstant> = cut_series
            .occurrences_in(EventRange {
                from: instant(NOV_9),
                to: instant("2026-12-07T10:00:00Z"),
            })
            .iter()
            .map(|occurrence| occurrence.starts_at)
            .collect();
        assert_eq!(shown_starts, vec![instant(NOV_9), instant(NOV_16)]);
    }

    #[sqlx::test]
    async fn removes_the_series_from_its_first_occurrence_on(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;

        let other_delete = delete(
            &db_pool,
            other_user_id,
            series_id,
            NOV_9,
            OccurrenceScope::All,
        )
        .await;
        delete(
            &db_pool,
            user_id,
            series_id,
            NOV_9,
            OccurrenceScope::Following,
        )
        .await
        .expect("the delete should run");

        assert!(matches!(other_delete, Err(CalendarEventError::NotFound)));
        assert_eq!(find_series(&db_pool, user_id, series_id).await, None);
    }
}
