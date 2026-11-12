use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::cut_off_series::CutOffSeries;
use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::data_sources::postgres::event_series_source::PostgresEventSeriesSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventChanges, CalendarEventKind, ChangedCalendarEvent, EventWrite,
    NamedTimeZone, NewCalendarEvent, parse_event_zone,
};
use crate::features::calendar_events::entities::event_series::{
    EventSeries, OccurrenceChange, OccurrenceRequest, OccurrenceScope,
};
use crate::features::calendar_events::error::CalendarEventError;

pub struct ChangeSeriesOccurrence<'a> {
    db_pool: &'a PgPool,
    cut_off_series: CutOffSeries,
    postgres_calendar_event_source: PostgresCalendarEventSource,
    postgres_event_series_source: PostgresEventSeriesSource,
}

struct SeriesTarget<'a> {
    user_id: Uuid,
    series: &'a EventSeries,
    original_start: UtcInstant,
    current_start: UtcInstant,
}

fn series_zone_of(
    changed_event: &ChangedCalendarEvent,
    input_zone: Option<&NamedTimeZone>,
) -> Result<NamedTimeZone, CalendarEventError> {
    match (&changed_event.time_zone, input_zone) {
        (Some(event_zone_name), _) => parse_event_zone(event_zone_name, "timeZone"),
        (None, Some(input_zone)) => Ok(input_zone.clone()),
        (None, None) => Err(CalendarEventError::Invalid(String::from(
            "a repeating event needs a time zone or an inputZone",
        ))),
    }
}

fn split_by(
    original_starts: &[UtcInstant],
    keeps_start: impl Fn(UtcInstant) -> bool,
) -> (Vec<UtcInstant>, Vec<UtcInstant>) {
    original_starts
        .iter()
        .copied()
        .partition(|original_start| keeps_start(*original_start))
}

impl<'a> ChangeSeriesOccurrence<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            cut_off_series: CutOffSeries::new(),
            postgres_calendar_event_source: PostgresCalendarEventSource,
            postgres_event_series_source: PostgresEventSeriesSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        occurrence_request: &OccurrenceRequest<'_>,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let original_start = occurrence_request.original_start;
        let mut change_transaction = self.db_pool.begin().await?;
        let series = self
            .postgres_event_series_source
            .find_locked(
                &mut *change_transaction,
                user_id,
                occurrence_request.series_id,
            )
            .await?
            .ok_or(CalendarEventError::NotFound)?;
        let override_event = self
            .postgres_calendar_event_source
            .find_override_locked(&mut *change_transaction, user_id, series.id, original_start)
            .await?;
        if override_event.is_none() && !series.holds_open_occurrence(original_start) {
            return Err(CalendarEventError::NotFound);
        }
        let series_target = SeriesTarget {
            user_id,
            series: &series,
            original_start,
            current_start: override_event
                .as_ref()
                .map_or(original_start, |override_event| override_event.starts_at),
        };
        let widened_scope =
            series.widened_scope(occurrence_request.occurrence_scope, original_start);
        let changed_occurrence = match widened_scope {
            OccurrenceScope::Following | OccurrenceScope::All
                if occurrence_request.event_changes.stops_repeating() =>
            {
                self.stop_repeating(
                    &mut change_transaction,
                    &series_target,
                    override_event,
                    occurrence_request.event_changes,
                    widened_scope,
                )
                .await?
            }
            OccurrenceScope::This => {
                self.change_this(
                    &mut change_transaction,
                    &series_target,
                    override_event,
                    occurrence_request.event_changes,
                )
                .await?
            }
            OccurrenceScope::Following => {
                self.change_following(&mut change_transaction, &series_target, occurrence_request)
                    .await?
            }
            OccurrenceScope::All => {
                self.change_all(&mut change_transaction, &series_target, occurrence_request)
                    .await?
            }
        };
        change_transaction.commit().await?;
        Ok(changed_occurrence)
    }

    async fn stop_repeating(
        &self,
        change_transaction: &mut Transaction<'_, Postgres>,
        series_target: &SeriesTarget<'_>,
        override_event: Option<CalendarEvent>,
        event_changes: &CalendarEventChanges,
        widened_scope: OccurrenceScope,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let user_id = series_target.user_id;
        let series = series_target.series;
        let changed_event = override_event
            .unwrap_or_else(|| series.occurrence_at(series_target.original_start))
            .apply_changes(event_changes, UtcInstant::now())?;
        if widened_scope == OccurrenceScope::All {
            self.postgres_event_series_source
                .delete(&mut **change_transaction, user_id, series.id)
                .await?;
        } else {
            self.cut_off_series
                .execute(
                    change_transaction,
                    user_id,
                    series,
                    series_target.original_start,
                )
                .await?;
        }
        let plain_event_id = Uuid::now_v7();
        let event_write = self
            .postgres_calendar_event_source
            .insert(
                &mut **change_transaction,
                &NewCalendarEvent {
                    id: plain_event_id,
                    user_id,
                    kind: CalendarEventKind::Planned,
                    title: changed_event.title,
                    category_id: changed_event.category_id,
                    task_id: None,
                    card_id: None,
                    starts_at: changed_event.starts_at,
                    ends_at: changed_event.ends_at,
                    finished_at: None,
                    last_answered_at: None,
                    time_zone: changed_event.time_zone,
                    series_id: None,
                    original_start: None,
                },
            )
            .await?;
        if event_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        self.postgres_calendar_event_source
            .find_locked(&mut **change_transaction, user_id, plain_event_id)
            .await?
            .ok_or(CalendarEventError::NotFound)
    }

    async fn change_this(
        &self,
        change_transaction: &mut Transaction<'_, Postgres>,
        series_target: &SeriesTarget<'_>,
        override_event: Option<CalendarEvent>,
        event_changes: &CalendarEventChanges,
    ) -> Result<CalendarEvent, CalendarEventError> {
        if event_changes.repeat_change.is_some() {
            return Err(CalendarEventError::Invalid(String::from(
                "a repeat changes this and the following occurrences or all of them",
            )));
        }
        let user_id = series_target.user_id;
        let series = series_target.series;
        let original_start = series_target.original_start;
        let current_event = override_event
            .clone()
            .unwrap_or_else(|| series.occurrence_at(original_start));
        let changed_event = current_event.apply_changes(event_changes, UtcInstant::now())?;
        let event_write = match override_event {
            Some(override_event) => {
                self.postgres_calendar_event_source
                    .update(
                        &mut **change_transaction,
                        user_id,
                        override_event.id,
                        &changed_event,
                    )
                    .await?
            }
            None => {
                self.postgres_event_series_source
                    .insert_exception(
                        &mut **change_transaction,
                        user_id,
                        series.id,
                        original_start,
                    )
                    .await?;
                self.postgres_calendar_event_source
                    .insert(
                        &mut **change_transaction,
                        &NewCalendarEvent {
                            id: Uuid::now_v7(),
                            user_id,
                            kind: CalendarEventKind::Planned,
                            title: changed_event.title,
                            category_id: changed_event.category_id,
                            task_id: None,
                            card_id: None,
                            starts_at: changed_event.starts_at,
                            ends_at: changed_event.ends_at,
                            finished_at: None,
                            last_answered_at: None,
                            time_zone: changed_event.time_zone,
                            series_id: Some(series.id),
                            original_start: Some(original_start),
                        },
                    )
                    .await?
            }
        };
        if event_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        self.postgres_calendar_event_source
            .find_override_locked(
                &mut **change_transaction,
                user_id,
                series.id,
                original_start,
            )
            .await?
            .ok_or(CalendarEventError::NotFound)
    }

    async fn change_following(
        &self,
        change_transaction: &mut Transaction<'_, Postgres>,
        series_target: &SeriesTarget<'_>,
        occurrence_request: &OccurrenceRequest<'_>,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let user_id = series_target.user_id;
        let series = series_target.series;
        let original_start = series_target.original_start;
        let event_changes = occurrence_request.event_changes;
        let changed_event = series
            .occurrence_at(original_start)
            .apply_changes(event_changes, UtcInstant::now())?;
        let series_zone = series_zone_of(&changed_event, occurrence_request.input_zone)?;
        let new_following_series = series
            .reshaped(&OccurrenceChange {
                anchor_start: original_start,
                current_start: series_target.current_start,
                changed_event: &changed_event,
                is_start_changed: event_changes.starts_at.is_some(),
                repeat_rule: event_changes
                    .new_repeat()
                    .map(|event_repeat| event_repeat.repeat_rule.clone()),
                used_occurrence_count: series.count_before(original_start),
                series_zone: &series_zone,
            })?
            .into_new_series(user_id);
        let series_write = self
            .postgres_event_series_source
            .insert(&mut **change_transaction, &new_following_series)
            .await?;
        if series_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        self.postgres_event_series_source
            .update(
                &mut **change_transaction,
                user_id,
                &EventSeries {
                    until_instant: Some(original_start),
                    ..series.clone()
                },
            )
            .await?;
        let following_series = new_following_series.into_series();
        let carries_start = |carried_start: UtcInstant| {
            carried_start != original_start && following_series.produces(carried_start)
        };
        let override_starts = self
            .postgres_calendar_event_source
            .list_override_starts(&mut **change_transaction, series.id)
            .await?;
        let later_override_starts: Vec<UtcInstant> = override_starts
            .into_iter()
            .filter(|override_start| *override_start >= original_start)
            .collect();
        let later_cancelled_starts: Vec<UtcInstant> = series
            .cancelled_starts
            .iter()
            .copied()
            .filter(|cancelled_start| *cancelled_start >= original_start)
            .collect();
        let (carried_overrides, dropped_overrides) =
            split_by(&later_override_starts, carries_start);
        let (carried_cancellations, dropped_cancellations) =
            split_by(&later_cancelled_starts, carries_start);
        self.postgres_calendar_event_source
            .move_overrides(
                &mut **change_transaction,
                series.id,
                following_series.id,
                &carried_overrides,
            )
            .await?;
        self.postgres_calendar_event_source
            .delete_overrides(&mut **change_transaction, series.id, &dropped_overrides)
            .await?;
        self.postgres_event_series_source
            .move_exceptions(
                &mut **change_transaction,
                series.id,
                following_series.id,
                &carried_cancellations,
            )
            .await?;
        self.postgres_event_series_source
            .delete_exceptions(&mut **change_transaction, series.id, &dropped_cancellations)
            .await?;
        following_series.shown_occurrence_at(changed_event.starts_at)
    }

    async fn change_all(
        &self,
        change_transaction: &mut Transaction<'_, Postgres>,
        series_target: &SeriesTarget<'_>,
        occurrence_request: &OccurrenceRequest<'_>,
    ) -> Result<CalendarEvent, CalendarEventError> {
        let user_id = series_target.user_id;
        let series = series_target.series;
        let original_start = series_target.original_start;
        let event_changes = occurrence_request.event_changes;
        let changed_event = series
            .occurrence_at(original_start)
            .apply_changes(event_changes, UtcInstant::now())?;
        let series_zone = series_zone_of(&changed_event, occurrence_request.input_zone)?;
        let changed_series = series.reshaped(&OccurrenceChange {
            anchor_start: series.start_instant().ok_or(CalendarEventError::NotFound)?,
            current_start: series_target.current_start,
            changed_event: &changed_event,
            is_start_changed: event_changes.starts_at.is_some(),
            repeat_rule: event_changes
                .new_repeat()
                .map(|event_repeat| event_repeat.repeat_rule.clone()),
            used_occurrence_count: 0,
            series_zone: &series_zone,
        })?;
        let series_write = self
            .postgres_event_series_source
            .update(&mut **change_transaction, user_id, &changed_series)
            .await?;
        if series_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        let keeps_start = |kept_start: UtcInstant| {
            kept_start != original_start && changed_series.produces(kept_start)
        };
        let override_starts = self
            .postgres_calendar_event_source
            .list_override_starts(&mut **change_transaction, series.id)
            .await?;
        let (_, dropped_overrides) = split_by(&override_starts, keeps_start);
        let (_, dropped_cancellations) = split_by(&series.cancelled_starts, keeps_start);
        self.postgres_calendar_event_source
            .delete_overrides(&mut **change_transaction, series.id, &dropped_overrides)
            .await?;
        self.postgres_event_series_source
            .delete_exceptions(&mut **change_transaction, series.id, &dropped_cancellations)
            .await?;
        changed_series.shown_occurrence_at(changed_event.starts_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, seed_category_id, seed_user_id,
    };
    use crate::features::calendar_events::data_sources::postgres::event_series_source::tests::{
        new_weekly_series, seed_series,
    };
    use crate::features::calendar_events::entities::calendar_event::EventRange;

    const NOV_16: &str = "2026-11-16T10:00:00Z";
    const NOV_23: &str = "2026-11-23T10:00:00Z";
    const NOV_30: &str = "2026-11-30T10:00:00Z";

    fn renamed_to(new_title: &str) -> CalendarEventChanges {
        CalendarEventChanges {
            title: Some(String::from(new_title)),
            ..CalendarEventChanges::default()
        }
    }

    async fn change(
        db_pool: &PgPool,
        user_id: Uuid,
        series_id: Uuid,
        original_start: &str,
        occurrence_scope: OccurrenceScope,
        event_changes: &CalendarEventChanges,
    ) -> Result<CalendarEvent, CalendarEventError> {
        ChangeSeriesOccurrence::new(db_pool)
            .execute(
                user_id,
                &OccurrenceRequest {
                    series_id,
                    original_start: instant(original_start),
                    occurrence_scope,
                    event_changes,
                    input_zone: None,
                },
            )
            .await
    }

    fn three_weeks() -> EventRange {
        EventRange {
            from: instant("2026-11-15T16:00:00Z"),
            to: instant("2026-12-06T16:00:00Z"),
        }
    }

    async fn shown_titles(db_pool: &PgPool, user_id: Uuid) -> Vec<(String, String)> {
        let shown_series = PostgresEventSeriesSource
            .list_shown_in_range(db_pool, user_id, three_weeks())
            .await
            .expect("the list should run");
        let single_events = PostgresCalendarEventSource
            .list_in_range(db_pool, user_id, three_weeks())
            .await
            .expect("the list should run");
        let mut shown_events: Vec<CalendarEvent> = shown_series
            .iter()
            .flat_map(|series| series.occurrences_in(three_weeks()))
            .chain(single_events)
            .collect();
        shown_events.sort_by_key(|shown_event| shown_event.starts_at);
        shown_events
            .into_iter()
            .map(|shown_event| (shown_event.title, shown_event.starts_at.to_iso_text()))
            .collect()
    }

    #[sqlx::test]
    async fn replaces_one_occurrence_and_edits_the_replacement_again(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;

        let first_change = change(
            &db_pool,
            user_id,
            series_id,
            NOV_23,
            OccurrenceScope::This,
            &renamed_to("Swim"),
        )
        .await
        .expect("the change should run");
        let second_change = change(
            &db_pool,
            user_id,
            series_id,
            NOV_23,
            OccurrenceScope::This,
            &renamed_to("Run"),
        )
        .await
        .expect("the change should run");

        assert_eq!(first_change.id, second_change.id);
        assert_eq!(
            second_change.series.map(|series| series.original_start),
            Some(instant(NOV_23))
        );
        assert_eq!(
            shown_titles(&db_pool, user_id).await,
            vec![
                (String::from("Gym"), instant(NOV_16).to_iso_text()),
                (String::from("Run"), instant(NOV_23).to_iso_text()),
                (String::from("Gym"), instant(NOV_30).to_iso_text()),
            ]
        );
    }

    #[sqlx::test]
    async fn splits_the_series_and_carries_later_replacements_along(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        change(
            &db_pool,
            user_id,
            series_id,
            NOV_30,
            OccurrenceScope::This,
            &renamed_to("Long run"),
        )
        .await
        .expect("the change should run");

        let first_following = change(
            &db_pool,
            user_id,
            series_id,
            NOV_23,
            OccurrenceScope::Following,
            &renamed_to("Swim"),
        )
        .await
        .expect("the change should run");

        let following_series_id = first_following
            .series
            .as_ref()
            .map(|series| series.series_id)
            .expect("an occurrence of a series");
        assert_ne!(following_series_id, series_id);
        assert_eq!(
            PostgresCalendarEventSource
                .list_override_starts(&db_pool, following_series_id)
                .await
                .expect("the list should run"),
            vec![instant(NOV_30)]
        );
        assert_eq!(
            shown_titles(&db_pool, user_id).await,
            vec![
                (String::from("Gym"), instant(NOV_16).to_iso_text()),
                (String::from("Swim"), instant(NOV_23).to_iso_text()),
                (String::from("Long run"), instant(NOV_30).to_iso_text()),
            ]
        );
    }

    #[sqlx::test]
    async fn changes_nothing_when_a_change_to_all_is_refused(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        change(
            &db_pool,
            user_id,
            series_id,
            NOV_30,
            OccurrenceScope::This,
            &renamed_to("Long run"),
        )
        .await
        .expect("the change should run");

        let refused_change = change(
            &db_pool,
            user_id,
            series_id,
            NOV_23,
            OccurrenceScope::All,
            &CalendarEventChanges {
                category_change: Some(Some(other_category_id)),
                ..renamed_to("Swim")
            },
        )
        .await;

        assert!(matches!(refused_change, Err(CalendarEventError::NotFound)));
        assert_eq!(
            shown_titles(&db_pool, user_id).await,
            vec![
                (String::from("Gym"), instant(NOV_16).to_iso_text()),
                (String::from("Gym"), instant(NOV_23).to_iso_text()),
                (String::from("Long run"), instant(NOV_30).to_iso_text()),
            ]
        );
    }

    #[sqlx::test]
    async fn answers_not_found_for_a_cancelled_or_missing_occurrence(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let series_id = seed_series(&db_pool, &new_weekly_series(user_id)).await;
        PostgresEventSeriesSource
            .insert_exception(&db_pool, user_id, series_id, instant(NOV_23))
            .await
            .expect("the insert should run");

        for (original_start, wanted_series_id) in [
            (NOV_23, series_id),
            ("2026-11-17T10:00:00Z", series_id),
            (NOV_16, Uuid::now_v7()),
        ] {
            let missing_change = change(
                &db_pool,
                user_id,
                wanted_series_id,
                original_start,
                OccurrenceScope::This,
                &renamed_to("Swim"),
            )
            .await;
            assert!(
                matches!(missing_change, Err(CalendarEventError::NotFound)),
                "{original_start}"
            );
        }
    }
}
