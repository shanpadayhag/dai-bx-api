use jiff::civil::DateTime;
use uuid::Uuid;

use crate::core::recurrence::{
    RepeatRule, next_after, occurrences, parse_time_zone, to_instant, to_local,
};
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventChanges, CalendarEventKind, ChangedCalendarEvent, EventRange,
    NamedTimeZone, SeriesOccurrence,
};
use crate::features::calendar_events::error::CalendarEventError;

const MILLIS_PER_MINUTE: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceScope {
    This,
    Following,
    All,
}

impl OccurrenceScope {
    pub fn parse(scope_text: &str) -> Result<Self, CalendarEventError> {
        match scope_text {
            "this" => Ok(Self::This),
            "following" => Ok(Self::Following),
            "all" => Ok(Self::All),
            _ => Err(CalendarEventError::Invalid(String::from(
                "scope is this, following or all",
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OccurrenceRequest<'a> {
    pub series_id: Uuid,
    pub original_start: UtcInstant,
    pub occurrence_scope: OccurrenceScope,
    pub event_changes: &'a CalendarEventChanges,
    pub input_zone: Option<&'a NamedTimeZone>,
}

#[derive(Debug, Clone)]
pub struct OccurrenceChange<'a> {
    pub anchor_start: UtcInstant,
    pub current_start: UtcInstant,
    pub changed_event: &'a ChangedCalendarEvent,
    pub is_start_changed: bool,
    pub repeat_rule: Option<RepeatRule>,
    pub used_occurrence_count: i64,
    pub series_zone: &'a NamedTimeZone,
}

pub fn parse_occurrence_path(
    series_id_text: &str,
    original_start_text: &str,
) -> Result<(Uuid, UtcInstant), CalendarEventError> {
    let series_id = Uuid::parse_str(series_id_text).map_err(|_| CalendarEventError::NotFound)?;
    let original_start =
        UtcInstant::parse(original_start_text).ok_or(CalendarEventError::NotFound)?;
    Ok((series_id, original_start))
}

fn never_happens() -> CalendarEventError {
    CalendarEventError::Invalid(String::from("this repeat never happens"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSeries {
    pub id: Uuid,
    pub title: String,
    pub category_id: Option<Uuid>,
    pub time_zone: String,
    pub local_start: DateTime,
    pub duration_minutes: i32,
    pub repeat_rule: RepeatRule,
    pub until_instant: Option<UtcInstant>,
    pub cancelled_starts: Vec<UtcInstant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEventSeries {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub category_id: Option<Uuid>,
    pub time_zone: String,
    pub local_start: DateTime,
    pub duration_minutes: i32,
    pub repeat_rule: RepeatRule,
    pub until_instant: Option<UtcInstant>,
}

impl NewEventSeries {
    pub fn starting_at(
        user_id: Uuid,
        title: &str,
        category_id: Option<Uuid>,
        starts_at: UtcInstant,
        ends_at: UtcInstant,
        series_zone: &NamedTimeZone,
        repeat_rule: RepeatRule,
    ) -> Result<Self, CalendarEventError> {
        let duration_minutes =
            i32::try_from((ends_at.unix_millis() - starts_at.unix_millis()) / MILLIS_PER_MINUTE)
                .map_err(|_| never_happens())?;
        let local_start = to_local(starts_at, &series_zone.time_zone).ok_or_else(never_happens)?;
        let local_start_instant =
            to_instant(local_start, &series_zone.time_zone).ok_or_else(never_happens)?;
        next_after(
            &repeat_rule,
            local_start,
            &series_zone.time_zone,
            UtcInstant::from_unix_millis(local_start_instant.unix_millis() - 1),
        )
        .ok_or_else(never_happens)?;
        Ok(Self {
            id: Uuid::now_v7(),
            user_id,
            title: title.to_owned(),
            category_id,
            time_zone: series_zone.name.clone(),
            local_start,
            duration_minutes,
            repeat_rule,
            until_instant: None,
        })
    }

    pub fn into_series(self) -> EventSeries {
        EventSeries {
            id: self.id,
            title: self.title,
            category_id: self.category_id,
            time_zone: self.time_zone,
            local_start: self.local_start,
            duration_minutes: self.duration_minutes,
            repeat_rule: self.repeat_rule,
            until_instant: self.until_instant,
            cancelled_starts: Vec::new(),
        }
    }
}

impl EventSeries {
    pub fn occurrences_in(&self, event_range: EventRange) -> Vec<CalendarEvent> {
        let Some(series_zone) = parse_time_zone(&self.time_zone) else {
            return Vec::new();
        };
        let earliest_start = UtcInstant::from_unix_millis(
            event_range.from.unix_millis() - self.duration_millis() + 1,
        );
        let latest_start = self.until_instant.map_or(event_range.to, |until_instant| {
            until_instant.min(event_range.to)
        });
        occurrences(
            &self.repeat_rule,
            self.local_start,
            &series_zone,
            earliest_start,
            latest_start,
        )
        .into_iter()
        .filter(|occurrence_start| !self.cancelled_starts.contains(occurrence_start))
        .map(|occurrence_start| self.occurrence_at(occurrence_start))
        .collect()
    }

    pub fn first_occurrence(&self) -> Option<CalendarEvent> {
        let series_zone = parse_time_zone(&self.time_zone)?;
        let local_start_instant = to_instant(self.local_start, &series_zone)?;
        let first_start = next_after(
            &self.repeat_rule,
            self.local_start,
            &series_zone,
            UtcInstant::from_unix_millis(local_start_instant.unix_millis() - 1),
        )?;
        let is_before_until = self
            .until_instant
            .is_none_or(|until_instant| first_start < until_instant);
        is_before_until.then(|| self.occurrence_at(first_start))
    }

    pub fn produces(&self, occurrence_start: UtcInstant) -> bool {
        let is_before_until = self
            .until_instant
            .is_none_or(|until_instant| occurrence_start < until_instant);
        is_before_until
            && parse_time_zone(&self.time_zone).is_some_and(|series_zone| {
                occurrences(
                    &self.repeat_rule,
                    self.local_start,
                    &series_zone,
                    occurrence_start,
                    UtcInstant::from_unix_millis(occurrence_start.unix_millis() + 1),
                ) == [occurrence_start]
            })
    }

    pub fn holds_open_occurrence(&self, occurrence_start: UtcInstant) -> bool {
        self.produces(occurrence_start) && !self.cancelled_starts.contains(&occurrence_start)
    }

    pub fn start_instant(&self) -> Option<UtcInstant> {
        to_instant(self.local_start, &parse_time_zone(&self.time_zone)?)
    }

    pub fn count_before(&self, occurrence_start: UtcInstant) -> i64 {
        let (Some(series_zone), Some(local_start_instant)) =
            (parse_time_zone(&self.time_zone), self.start_instant())
        else {
            return 0;
        };
        let earlier_starts = occurrences(
            &self.repeat_rule,
            self.local_start,
            &series_zone,
            local_start_instant,
            occurrence_start,
        );
        i64::try_from(earlier_starts.len()).unwrap_or(i64::MAX)
    }

    pub fn widened_scope(
        &self,
        occurrence_scope: OccurrenceScope,
        occurrence_start: UtcInstant,
    ) -> OccurrenceScope {
        if occurrence_scope == OccurrenceScope::Following
            && self.count_before(occurrence_start) == 0
        {
            OccurrenceScope::All
        } else {
            occurrence_scope
        }
    }

    pub fn shown_occurrence_at(
        &self,
        occurrence_start: UtcInstant,
    ) -> Result<CalendarEvent, CalendarEventError> {
        if self.produces(occurrence_start) {
            Ok(self.occurrence_at(occurrence_start))
        } else {
            self.first_occurrence().ok_or_else(never_happens)
        }
    }

    pub fn reshaped(
        &self,
        occurrence_change: &OccurrenceChange<'_>,
    ) -> Result<EventSeries, CalendarEventError> {
        let changed_event = occurrence_change.changed_event;
        let series_zone = occurrence_change.series_zone;
        let anchor_local = to_local(occurrence_change.anchor_start, &series_zone.time_zone)
            .ok_or_else(never_happens)?;
        let (local_start, shifted_day_count) = if occurrence_change.is_start_changed {
            let current_local = to_local(occurrence_change.current_start, &series_zone.time_zone)
                .ok_or_else(never_happens)?;
            let changed_local = to_local(changed_event.starts_at, &series_zone.time_zone)
                .ok_or_else(never_happens)?;
            let civil_shift = current_local
                .until(changed_local)
                .map_err(|_| never_happens())?;
            let shifted_day_count = current_local
                .date()
                .until(changed_local.date())
                .map_err(|_| never_happens())?
                .get_days();
            (
                anchor_local
                    .checked_add(civil_shift)
                    .map_err(|_| never_happens())?,
                i64::from(shifted_day_count),
            )
        } else {
            (anchor_local, 0)
        };
        let ends_at = changed_event.ends_at.unwrap_or(changed_event.starts_at);
        let reshaped_series = EventSeries {
            id: self.id,
            title: changed_event.title.clone(),
            category_id: changed_event.category_id,
            time_zone: series_zone.name.clone(),
            local_start,
            duration_minutes: i32::try_from(
                (ends_at.unix_millis() - changed_event.starts_at.unix_millis()) / MILLIS_PER_MINUTE,
            )
            .map_err(|_| never_happens())?,
            repeat_rule: occurrence_change.repeat_rule.clone().unwrap_or_else(|| {
                self.repeat_rule
                    .shifted_by_days(shifted_day_count)
                    .after_used_occurrences(occurrence_change.used_occurrence_count)
            }),
            until_instant: self.until_instant,
            cancelled_starts: Vec::new(),
        };
        reshaped_series
            .first_occurrence()
            .ok_or_else(never_happens)?;
        Ok(reshaped_series)
    }

    pub fn into_new_series(self, user_id: Uuid) -> NewEventSeries {
        NewEventSeries {
            id: Uuid::now_v7(),
            user_id,
            title: self.title,
            category_id: self.category_id,
            time_zone: self.time_zone,
            local_start: self.local_start,
            duration_minutes: self.duration_minutes,
            repeat_rule: self.repeat_rule,
            until_instant: self.until_instant,
        }
    }

    fn duration_millis(&self) -> i64 {
        i64::from(self.duration_minutes) * MILLIS_PER_MINUTE
    }

    pub fn occurrence_at(&self, occurrence_start: UtcInstant) -> CalendarEvent {
        CalendarEvent {
            id: self.id,
            kind: CalendarEventKind::Planned,
            title: self.title.clone(),
            category_id: self.category_id,
            source: None,
            starts_at: occurrence_start,
            ends_at: Some(UtcInstant::from_unix_millis(
                occurrence_start.unix_millis() + self.duration_millis(),
            )),
            finished_at: None,
            last_answered_at: None,
            time_zone: Some(self.time_zone.clone()),
            series: Some(SeriesOccurrence {
                series_id: self.id,
                original_start: occurrence_start,
                repeat_rule: self.repeat_rule.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::entities::calendar_event::parse_event_zone;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

    fn daily_series() -> EventSeries {
        EventSeries {
            id: Uuid::now_v7(),
            title: String::from("Morning run"),
            category_id: None,
            time_zone: String::from("Asia/Manila"),
            local_start: "2026-11-09T07:00:00".parse().expect("a local time"),
            duration_minutes: 45,
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
            )
            .expect("a valid rule"),
            until_instant: None,
            cancelled_starts: Vec::new(),
        }
    }

    fn manila_week() -> EventRange {
        EventRange {
            from: instant("2026-11-08T16:00:00Z"),
            to: instant("2026-11-15T16:00:00Z"),
        }
    }

    fn starts_of(listed_events: &[CalendarEvent]) -> Vec<String> {
        listed_events
            .iter()
            .map(|listed_event| listed_event.starts_at.to_iso_text())
            .collect()
    }

    #[test]
    fn expands_into_planned_events_carrying_the_series() {
        let series = daily_series();

        let week_events = series.occurrences_in(manila_week());

        assert_eq!(week_events.len(), 7);
        let first_event = &week_events[0];
        assert_eq!(first_event.kind, CalendarEventKind::Planned);
        assert_eq!(first_event.title, "Morning run");
        assert_eq!(first_event.starts_at, instant("2026-11-08T23:00:00Z"));
        assert_eq!(first_event.ends_at, Some(instant("2026-11-08T23:45:00Z")));
        assert_eq!(first_event.time_zone.as_deref(), Some("Asia/Manila"));
        assert_eq!(
            first_event.series,
            Some(SeriesOccurrence {
                series_id: series.id,
                original_start: instant("2026-11-08T23:00:00Z"),
                repeat_rule: series.repeat_rule.clone(),
            })
        );
    }

    #[test]
    fn includes_an_occurrence_that_started_before_the_range_and_runs_into_it() {
        let series = daily_series();

        let overlapping_events = series.occurrences_in(EventRange {
            from: instant("2026-11-08T23:30:00Z"),
            to: instant("2026-11-09T00:00:00Z"),
        });
        let after_it_ended = series.occurrences_in(EventRange {
            from: instant("2026-11-08T23:45:00Z"),
            to: instant("2026-11-09T00:00:00Z"),
        });

        assert_eq!(starts_of(&overlapping_events), ["2026-11-08T23:00:00.000Z"]);
        assert!(after_it_ended.is_empty());
    }

    #[test]
    fn leaves_out_cancelled_occurrences_and_those_from_its_cut_off_on() {
        let series = EventSeries {
            cancelled_starts: vec![instant("2026-11-10T23:00:00Z")],
            until_instant: Some(instant("2026-11-12T23:00:00Z")),
            ..daily_series()
        };

        assert_eq!(
            starts_of(&series.occurrences_in(manila_week())),
            [
                "2026-11-08T23:00:00.000Z",
                "2026-11-09T23:00:00.000Z",
                "2026-11-11T23:00:00.000Z",
            ]
        );
    }

    #[test]
    fn finds_its_first_occurrence_on_the_first_repeat_day() {
        let weekly_series = EventSeries {
            local_start: "2026-11-11T07:00:00".parse().expect("a local time"),
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
            )
            .expect("a valid rule"),
            ..daily_series()
        };
        let ended_series = EventSeries {
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2026-11-08"}}"#,
            )
            .expect("a valid rule"),
            ..daily_series()
        };

        assert_eq!(
            weekly_series
                .first_occurrence()
                .map(|first_event| first_event.starts_at),
            Some(instant("2026-11-15T23:00:00Z"))
        );
        assert_eq!(ended_series.first_occurrence(), None);
    }

    fn weekly_monday_series() -> EventSeries {
        EventSeries {
            local_start: "2026-11-09T09:00:00".parse().expect("a local time"),
            time_zone: String::from("UTC"),
            duration_minutes: 60,
            repeat_rule: serde_json::from_str(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"after":4}}"#,
            )
            .expect("a valid rule"),
            ..daily_series()
        }
    }

    fn changed_to(starts_at: &str, ends_at: &str) -> ChangedCalendarEvent {
        ChangedCalendarEvent {
            title: String::from("Gym"),
            category_id: None,
            starts_at: instant(starts_at),
            ends_at: Some(instant(ends_at)),
            finished_at: None,
            time_zone: Some(String::from("UTC")),
        }
    }

    #[test]
    fn knows_which_starts_it_produces_and_how_many_came_before() {
        let series = EventSeries {
            until_instant: Some(instant("2026-11-23T09:00:00Z")),
            ..weekly_monday_series()
        };

        assert!(series.produces(instant("2026-11-16T09:00:00Z")));
        assert!(!series.produces(instant("2026-11-16T09:30:00Z")));
        assert!(!series.produces(instant("2026-11-02T09:00:00Z")));
        assert!(!series.produces(instant("2026-11-23T09:00:00Z")));
        assert_eq!(series.count_before(instant("2026-11-23T09:00:00Z")), 2);
        assert_eq!(series.count_before(instant("2026-11-09T09:00:00Z")), 0);
    }

    #[test]
    fn moves_the_whole_series_and_its_weekday_with_a_moved_occurrence() {
        let series = weekly_monday_series();
        let utc = parse_event_zone("UTC", "inputZone").expect("a known zone");
        let changed_event = changed_to("2026-12-01T10:00:00Z", "2026-12-01T10:30:00Z");

        let reshaped_series = series
            .reshaped(&OccurrenceChange {
                anchor_start: instant("2026-11-09T09:00:00Z"),
                current_start: instant("2026-11-30T09:00:00Z"),
                changed_event: &changed_event,
                is_start_changed: true,
                repeat_rule: None,
                used_occurrence_count: 0,
                series_zone: &utc,
            })
            .expect("the series should reshape");

        assert_eq!(
            reshaped_series.local_start,
            "2026-11-10T10:00:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(reshaped_series.duration_minutes, 30);
        assert_eq!(reshaped_series.id, series.id);
        assert_eq!(
            serde_json::to_value(&reshaped_series.repeat_rule).expect("rule writes")["weekdays"],
            serde_json::json!(["tue"])
        );
    }

    #[test]
    fn starts_the_following_part_at_the_split_with_the_remaining_count() {
        let series = weekly_monday_series();
        let utc = parse_event_zone("UTC", "inputZone").expect("a known zone");
        let changed_event = changed_to("2026-11-23T09:00:00Z", "2026-11-23T10:00:00Z");

        let following_series = series
            .reshaped(&OccurrenceChange {
                anchor_start: instant("2026-11-23T09:00:00Z"),
                current_start: instant("2026-11-23T09:00:00Z"),
                changed_event: &changed_event,
                is_start_changed: false,
                repeat_rule: None,
                used_occurrence_count: 2,
                series_zone: &utc,
            })
            .expect("the series should reshape")
            .into_new_series(Uuid::now_v7());

        assert_ne!(following_series.id, series.id);
        assert_eq!(
            following_series.local_start,
            "2026-11-23T09:00:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(
            serde_json::to_value(&following_series.repeat_rule).expect("rule writes")["ends"],
            serde_json::json!({ "after": 2 })
        );
    }

    #[test]
    fn widens_a_following_change_from_the_first_occurrence_to_all() {
        let series = EventSeries {
            cancelled_starts: vec![instant("2026-11-16T09:00:00Z")],
            ..weekly_monday_series()
        };

        assert_eq!(
            series.widened_scope(OccurrenceScope::Following, instant("2026-11-09T09:00:00Z")),
            OccurrenceScope::All
        );
        assert_eq!(
            series.widened_scope(OccurrenceScope::Following, instant("2026-11-23T09:00:00Z")),
            OccurrenceScope::Following
        );
        assert_eq!(
            series.widened_scope(OccurrenceScope::This, instant("2026-11-09T09:00:00Z")),
            OccurrenceScope::This
        );
        assert!(series.holds_open_occurrence(instant("2026-11-23T09:00:00Z")));
        assert!(!series.holds_open_occurrence(instant("2026-11-16T09:00:00Z")));
    }

    #[test]
    fn reads_only_the_three_scopes() {
        assert_eq!(
            OccurrenceScope::parse("this").ok(),
            Some(OccurrenceScope::This)
        );
        assert_eq!(
            OccurrenceScope::parse("following").ok(),
            Some(OccurrenceScope::Following)
        );
        assert_eq!(
            OccurrenceScope::parse("all").ok(),
            Some(OccurrenceScope::All)
        );
        for refused_scope in ["", "This", "some"] {
            assert!(
                OccurrenceScope::parse(refused_scope).is_err(),
                "{refused_scope}"
            );
        }
    }

    #[test]
    fn starts_a_new_series_at_the_local_time_in_its_own_zone() {
        let london = parse_event_zone("Europe/London", "timeZone").expect("a known zone");

        let new_series = NewEventSeries::starting_at(
            Uuid::now_v7(),
            "Call home",
            None,
            instant("2026-11-09T11:00:00Z"),
            instant("2026-11-09T11:30:00Z"),
            &london,
            daily_series().repeat_rule,
        )
        .expect("the series should be built");
        let never_happens = NewEventSeries::starting_at(
            Uuid::now_v7(),
            "Call home",
            None,
            instant("2026-11-09T11:00:00Z"),
            instant("2026-11-09T11:30:00Z"),
            &london,
            serde_json::from_str(
                r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2026-11-08"}}"#,
            )
            .expect("a valid rule"),
        );

        assert_eq!(
            new_series.local_start,
            "2026-11-09T11:00:00"
                .parse::<DateTime>()
                .expect("a local time")
        );
        assert_eq!(new_series.duration_minutes, 30);
        assert_eq!(new_series.time_zone, "Europe/London");
        assert!(matches!(never_happens, Err(CalendarEventError::Invalid(_))));
    }
}
