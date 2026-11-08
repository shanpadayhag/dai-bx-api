use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use uuid::Uuid;

use crate::core::recurrence::{
    RepeatRule, parse_calendar_date, parse_local_moment, parse_time_zone, to_instant,
};
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::card_schedules::entities::card_schedule::ScheduledMove;

pub const PLANNED_TITLE_MAX_CHARACTERS: usize = 200;
pub const PLANNED_MINIMUM_MINUTES: i64 = 15;
pub const RANGE_DAY_COUNTS: [i64; 2] = [1, 7];
pub const FINISH_CLOCK_SKEW_MINUTES: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarEventKind {
    Planned,
    Tracked,
}

impl CalendarEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarEventKind::Planned => "planned",
            CalendarEventKind::Tracked => "tracked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarEventSource {
    Task { task_id: Uuid, group_id: Uuid },
    Card { card_id: Uuid, board_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesOccurrence {
    pub series_id: Uuid,
    pub original_start: UtcInstant,
    pub repeat_rule: RepeatRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub kind: CalendarEventKind,
    pub title: String,
    pub category_id: Option<Uuid>,
    pub source: Option<CalendarEventSource>,
    pub starts_at: UtcInstant,
    pub ends_at: Option<UtcInstant>,
    pub finished_at: Option<UtcInstant>,
    pub last_answered_at: Option<UtcInstant>,
    pub time_zone: Option<String>,
    pub series: Option<SeriesOccurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCalendarEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: CalendarEventKind,
    pub title: String,
    pub category_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub card_id: Option<Uuid>,
    pub starts_at: UtcInstant,
    pub ends_at: Option<UtcInstant>,
    pub finished_at: Option<UtcInstant>,
    pub last_answered_at: Option<UtcInstant>,
    pub time_zone: Option<String>,
    pub series_id: Option<Uuid>,
    pub original_start: Option<UtcInstant>,
}

#[derive(Debug, Clone)]
pub struct NamedTimeZone {
    pub name: String,
    pub time_zone: TimeZone,
}

#[derive(Debug, Clone)]
pub struct EventRepeat {
    pub repeat_rule: RepeatRule,
    pub input_zone: NamedTimeZone,
}

impl EventRepeat {
    pub fn series_zone(
        &self,
        event_zone_name: Option<&str>,
    ) -> Result<NamedTimeZone, CalendarEventError> {
        event_zone_name.map_or_else(
            || Ok(self.input_zone.clone()),
            |event_zone_name| parse_event_zone(event_zone_name, "timeZone"),
        )
    }
}

#[derive(Debug, Clone)]
pub struct PlannedEventInput {
    pub title: String,
    pub category_id: Option<Uuid>,
    pub starts_at: UtcInstant,
    pub ends_at: UtcInstant,
    pub time_zone: Option<String>,
    pub repeat: Option<EventRepeat>,
}

#[derive(Debug, Clone, Default)]
pub struct CalendarEventChanges {
    pub title: Option<String>,
    pub category_change: Option<Option<Uuid>>,
    pub starts_at: Option<UtcInstant>,
    pub ends_at: Option<UtcInstant>,
    pub time_zone_change: Option<Option<String>>,
    pub repeat: Option<EventRepeat>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedCalendarEvent {
    pub title: String,
    pub category_id: Option<Uuid>,
    pub starts_at: UtcInstant,
    pub ends_at: Option<UtcInstant>,
    pub finished_at: Option<UtcInstant>,
    pub time_zone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarRange {
    pub events: Vec<CalendarEvent>,
    pub moves: Vec<ScheduledMove>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventRange {
    pub from: UtcInstant,
    pub to: UtcInstant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventWrite {
    Written,
    UnknownLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedSubject {
    Task(Uuid),
    Card(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunningEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub subject: TrackedSubject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosingScope {
    Task(Uuid),
    TaskGroup(Uuid),
    Card(Uuid),
    BoardList(Uuid),
    Board(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningOpen {
    Opened,
    AlreadyRunning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningItem {
    pub event: CalendarEvent,
    pub estimate_minutes: Option<i32>,
    pub source_name: Option<String>,
}

impl CalendarEvent {
    pub fn is_running(&self) -> bool {
        self.kind == CalendarEventKind::Tracked && self.ends_at.is_none()
    }

    pub fn task_id(&self) -> Option<Uuid> {
        match self.source {
            Some(CalendarEventSource::Task { task_id, .. }) => Some(task_id),
            _ => None,
        }
    }

    pub fn finish(
        &self,
        requested_ends_at: UtcInstant,
        now: UtcInstant,
    ) -> Result<ChangedCalendarEvent, CalendarEventError> {
        if !self.is_running() {
            return Err(invalid("this event has already finished"));
        }
        if requested_ends_at > now.plus_minutes(FINISH_CLOCK_SKEW_MINUTES) {
            return Err(invalid("a tracked event can't end in the future"));
        }
        let ends_at = requested_ends_at.min(now);
        if ends_at <= self.starts_at {
            return Err(invalid("an event ends after it starts"));
        }
        Ok(ChangedCalendarEvent {
            title: self.title.clone(),
            category_id: self.category_id,
            starts_at: self.starts_at,
            ends_at: Some(ends_at),
            finished_at: Some(now),
            time_zone: None,
        })
    }

    pub fn apply_changes(
        &self,
        event_changes: &CalendarEventChanges,
        now: UtcInstant,
    ) -> Result<ChangedCalendarEvent, CalendarEventError> {
        let category_id = event_changes.category_change.unwrap_or(self.category_id);
        let starts_at = event_changes.starts_at.unwrap_or(self.starts_at);
        match self.kind {
            CalendarEventKind::Planned => {
                let ends_at = event_changes.ends_at.or(self.ends_at).unwrap_or(starts_at);
                validate_planned_times(starts_at, ends_at)?;
                Ok(ChangedCalendarEvent {
                    title: event_changes
                        .title
                        .clone()
                        .unwrap_or_else(|| self.title.clone()),
                    category_id,
                    starts_at,
                    ends_at: Some(ends_at),
                    finished_at: None,
                    time_zone: event_changes
                        .time_zone_change
                        .clone()
                        .unwrap_or_else(|| self.time_zone.clone()),
                })
            }
            CalendarEventKind::Tracked => {
                if event_changes.title.is_some() {
                    return Err(invalid("a tracked event keeps its task's or card's name"));
                }
                if event_changes.time_zone_change.is_some() || event_changes.repeat.is_some() {
                    return Err(invalid(
                        "a tracked event shows in the calendar's zone and doesn't repeat",
                    ));
                }
                let changes_times =
                    event_changes.starts_at.is_some() || event_changes.ends_at.is_some();
                let Some(current_ends_at) = self.ends_at else {
                    if changes_times {
                        return Err(invalid("finish a running event before changing its times"));
                    }
                    return Ok(ChangedCalendarEvent {
                        title: self.title.clone(),
                        category_id,
                        starts_at,
                        ends_at: None,
                        finished_at: None,
                        time_zone: None,
                    });
                };
                let ends_at = event_changes.ends_at.unwrap_or(current_ends_at);
                if ends_at <= starts_at {
                    return Err(invalid("an event ends after it starts"));
                }
                if changes_times && ends_at > now {
                    return Err(invalid("a tracked event can't end in the future"));
                }
                let finished_at = self.finished_at.map(|finished_at| finished_at.max(ends_at));
                Ok(ChangedCalendarEvent {
                    title: self.title.clone(),
                    category_id,
                    starts_at,
                    ends_at: Some(ends_at),
                    finished_at,
                    time_zone: None,
                })
            }
        }
    }
}

fn invalid(invalid_message: &str) -> CalendarEventError {
    CalendarEventError::Invalid(invalid_message.to_owned())
}

pub fn validate_planned_title(raw_title: &str) -> Result<String, CalendarEventError> {
    let trimmed_title = raw_title.trim();
    let title_character_count = trimmed_title.chars().count();
    if title_character_count == 0 || title_character_count > PLANNED_TITLE_MAX_CHARACTERS {
        return Err(invalid("an event title is 1 to 200 characters"));
    }
    Ok(trimmed_title.to_owned())
}

pub fn parse_event_instant(
    instant_text: &str,
    field_name: &str,
) -> Result<UtcInstant, CalendarEventError> {
    UtcInstant::parse(instant_text).ok_or_else(|| {
        CalendarEventError::Invalid(format!(
            "{field_name} is a UTC time like 2026-11-05T09:30:00Z"
        ))
    })
}

pub fn parse_event_zone(
    zone_name: &str,
    field_name: &str,
) -> Result<NamedTimeZone, CalendarEventError> {
    let time_zone = parse_time_zone(zone_name).ok_or_else(|| {
        CalendarEventError::Invalid(format!("{field_name} is a time zone name like Asia/Manila"))
    })?;
    Ok(NamedTimeZone {
        name: zone_name.to_owned(),
        time_zone,
    })
}

pub fn parse_event_moment(
    moment_text: &str,
    field_name: &str,
    input_zone: &NamedTimeZone,
) -> Result<UtcInstant, CalendarEventError> {
    parse_local_moment(moment_text)
        .and_then(|local_moment| to_instant(local_moment, &input_zone.time_zone))
        .ok_or_else(|| {
            CalendarEventError::Invalid(format!(
                "{field_name} is a local time like 2026-11-09T18:00"
            ))
        })
}

pub fn validate_planned_times(
    starts_at: UtcInstant,
    ends_at: UtcInstant,
) -> Result<(), CalendarEventError> {
    if ends_at <= starts_at {
        return Err(invalid("an event ends after it starts"));
    }
    if ends_at < starts_at.plus_minutes(PLANNED_MINIMUM_MINUTES) {
        return Err(invalid("a planned event lasts at least 15 minutes"));
    }
    Ok(())
}

pub fn validate_event_range(
    start_text: &str,
    day_count_text: &str,
    range_zone: &NamedTimeZone,
) -> Result<EventRange, CalendarEventError> {
    let start_date = parse_calendar_date(start_text)
        .ok_or_else(|| invalid("start is a date like 2026-11-09"))?;
    let day_count = day_count_text
        .parse::<i64>()
        .ok()
        .filter(|day_count| RANGE_DAY_COUNTS.contains(day_count))
        .ok_or_else(|| invalid("days is 1 or 7"))?;
    let end_date = start_date
        .checked_add(jiff::Span::new().days(day_count))
        .map_err(|_| invalid("start is a date like 2026-11-09"))?;
    let midnight_instant = |range_date: Date| {
        to_instant(
            range_date.to_datetime(Time::midnight()),
            &range_zone.time_zone,
        )
        .ok_or_else(|| invalid("start is a date like 2026-11-09"))
    };
    Ok(EventRange {
        from: midnight_instant(start_date)?,
        to: midnight_instant(end_date)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

    fn tracked_event(ends_at: Option<&str>, finished_at: Option<&str>) -> CalendarEvent {
        CalendarEvent {
            id: Uuid::now_v7(),
            kind: CalendarEventKind::Tracked,
            title: String::from("Write the changelog"),
            category_id: None,
            source: None,
            starts_at: instant("2026-11-05T09:00:00Z"),
            ends_at: ends_at.map(instant),
            finished_at: finished_at.map(instant),
            last_answered_at: None,
            time_zone: None,
            series: None,
        }
    }

    fn planned_event() -> CalendarEvent {
        CalendarEvent {
            kind: CalendarEventKind::Planned,
            title: String::from("Gym"),
            ends_at: Some(instant("2026-11-05T10:00:00Z")),
            finished_at: None,
            ..tracked_event(None, None)
        }
    }

    const NOW: &str = "2026-11-05T12:00:00Z";

    #[test]
    fn a_tracked_event_without_an_end_is_running() {
        assert!(tracked_event(None, None).is_running());
        assert!(
            !tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T09:40:00Z")).is_running()
        );
        assert!(!planned_event().is_running());
    }

    #[test]
    fn a_finish_keeps_a_chosen_end_and_stamps_now() {
        let finished_event = tracked_event(None, None)
            .finish(instant("2026-11-05T09:40:00Z"), instant(NOW))
            .expect("the finish should be allowed");

        assert_eq!(
            finished_event.ends_at,
            Some(instant("2026-11-05T09:40:00Z"))
        );
        assert_eq!(finished_event.finished_at, Some(instant(NOW)));
        assert_eq!(finished_event.starts_at, instant("2026-11-05T09:00:00Z"));
    }

    #[test]
    fn a_finish_up_to_two_minutes_ahead_ends_at_now() {
        for requested_ends_at in ["2026-11-05T12:00:30Z", "2026-11-05T12:02:00Z"] {
            let finished_event = tracked_event(None, None)
                .finish(instant(requested_ends_at), instant(NOW))
                .expect("the finish should be allowed");
            assert_eq!(finished_event.ends_at, Some(instant(NOW)));
        }
    }

    #[test]
    fn a_finish_refuses_an_end_outside_the_start_and_now() {
        for refused_ends_at in [
            "2026-11-05T08:59:59Z",
            "2026-11-05T09:00:00Z",
            "2026-11-05T12:02:00.001Z",
        ] {
            let finish_result =
                tracked_event(None, None).finish(instant(refused_ends_at), instant(NOW));
            assert!(
                matches!(finish_result, Err(CalendarEventError::Invalid(_))),
                "{refused_ends_at}"
            );
        }
    }

    #[test]
    fn a_finish_refuses_a_finished_or_planned_event() {
        for refused_event in [
            tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T09:40:00Z")),
            planned_event(),
        ] {
            let finish_result = refused_event.finish(instant("2026-11-05T11:00:00Z"), instant(NOW));
            assert!(matches!(finish_result, Err(CalendarEventError::Invalid(_))));
        }
    }

    #[test]
    fn only_a_task_event_has_a_task_id() {
        let task_id = Uuid::now_v7();
        let task_event = CalendarEvent {
            source: Some(CalendarEventSource::Task {
                task_id,
                group_id: Uuid::now_v7(),
            }),
            ..tracked_event(None, None)
        };
        let card_event = CalendarEvent {
            source: Some(CalendarEventSource::Card {
                card_id: Uuid::now_v7(),
                board_id: Uuid::now_v7(),
            }),
            ..tracked_event(None, None)
        };

        assert_eq!(task_event.task_id(), Some(task_id));
        assert_eq!(card_event.task_id(), None);
        assert_eq!(tracked_event(None, None).task_id(), None);
    }

    #[test]
    fn a_planned_event_takes_every_change() {
        let category_id = Uuid::now_v7();

        let changed_event = planned_event()
            .apply_changes(
                &CalendarEventChanges {
                    title: Some(String::from("Swim")),
                    category_change: Some(Some(category_id)),
                    starts_at: Some(instant("2026-11-06T18:00:00Z")),
                    ends_at: Some(instant("2026-11-08T19:00:00Z")),
                    time_zone_change: Some(Some(String::from("Europe/London"))),
                    repeat: None,
                },
                instant(NOW),
            )
            .expect("the change should be allowed");

        assert_eq!(
            changed_event,
            ChangedCalendarEvent {
                title: String::from("Swim"),
                category_id: Some(category_id),
                starts_at: instant("2026-11-06T18:00:00Z"),
                ends_at: Some(instant("2026-11-08T19:00:00Z")),
                finished_at: None,
                time_zone: Some(String::from("Europe/London")),
            }
        );
    }

    #[test]
    fn a_planned_event_keeps_at_least_15_minutes() {
        let too_short = planned_event().apply_changes(
            &CalendarEventChanges {
                ends_at: Some(instant("2026-11-05T09:14:59Z")),
                ..CalendarEventChanges::default()
            },
            instant(NOW),
        );
        let exactly_fifteen = planned_event().apply_changes(
            &CalendarEventChanges {
                ends_at: Some(instant("2026-11-05T09:15:00Z")),
                ..CalendarEventChanges::default()
            },
            instant(NOW),
        );

        assert!(matches!(too_short, Err(CalendarEventError::Invalid(_))));
        assert!(exactly_fifteen.is_ok());
    }

    #[test]
    fn a_planned_event_can_end_in_the_future() {
        let next_month = planned_event().apply_changes(
            &CalendarEventChanges {
                starts_at: Some(instant("2026-12-05T09:00:00Z")),
                ends_at: Some(instant("2026-12-05T10:00:00Z")),
                ..CalendarEventChanges::default()
            },
            instant(NOW),
        );

        assert!(next_month.is_ok());
    }

    #[test]
    fn a_tracked_event_keeps_its_name() {
        let renamed = tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T09:40:00Z"))
            .apply_changes(
                &CalendarEventChanges {
                    title: Some(String::from("Renamed")),
                    ..CalendarEventChanges::default()
                },
                instant(NOW),
            );

        assert!(matches!(renamed, Err(CalendarEventError::Invalid(_))));
    }

    #[test]
    fn a_running_event_changes_only_its_category() {
        let category_id = Uuid::now_v7();
        let running_event = tracked_event(None, None);

        let recategorized = running_event
            .apply_changes(
                &CalendarEventChanges {
                    category_change: Some(Some(category_id)),
                    ..CalendarEventChanges::default()
                },
                instant(NOW),
            )
            .expect("a category change should be allowed");
        let retimed = running_event.apply_changes(
            &CalendarEventChanges {
                starts_at: Some(instant("2026-11-05T08:00:00Z")),
                ..CalendarEventChanges::default()
            },
            instant(NOW),
        );

        assert_eq!(recategorized.category_id, Some(category_id));
        assert_eq!(recategorized.ends_at, None);
        assert!(matches!(retimed, Err(CalendarEventError::Invalid(_))));
    }

    #[test]
    fn a_finished_event_moves_its_finish_along_when_the_end_passes_it() {
        let finished_event =
            tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T10:00:00Z"));

        let before_finish = finished_event
            .apply_changes(
                &CalendarEventChanges {
                    ends_at: Some(instant("2026-11-05T09:50:00Z")),
                    ..CalendarEventChanges::default()
                },
                instant(NOW),
            )
            .expect("an earlier end should be allowed");
        let past_finish = finished_event
            .apply_changes(
                &CalendarEventChanges {
                    ends_at: Some(instant("2026-11-05T10:30:00Z")),
                    ..CalendarEventChanges::default()
                },
                instant(NOW),
            )
            .expect("a later end should be allowed");

        assert_eq!(
            before_finish.finished_at,
            Some(instant("2026-11-05T10:00:00Z"))
        );
        assert_eq!(past_finish.ends_at, Some(instant("2026-11-05T10:30:00Z")));
        assert_eq!(
            past_finish.finished_at,
            Some(instant("2026-11-05T10:30:00Z"))
        );
    }

    #[test]
    fn a_finished_event_can_t_end_in_the_future_or_before_it_starts() {
        let finished_event =
            tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T10:00:00Z"));

        for rejected_changes in [
            CalendarEventChanges {
                ends_at: Some(instant("2026-11-05T12:00:00.001Z")),
                ..CalendarEventChanges::default()
            },
            CalendarEventChanges {
                ends_at: Some(instant("2026-11-05T09:00:00Z")),
                ..CalendarEventChanges::default()
            },
            CalendarEventChanges {
                starts_at: Some(instant("2026-11-05T09:40:00Z")),
                ..CalendarEventChanges::default()
            },
        ] {
            let rejected = finished_event.apply_changes(&rejected_changes, instant(NOW));
            assert!(
                matches!(rejected, Err(CalendarEventError::Invalid(_))),
                "{rejected_changes:?}"
            );
        }
    }

    #[test]
    fn a_tracked_event_may_have_a_short_run() {
        let short_run = tracked_event(Some("2026-11-05T09:02:00Z"), Some("2026-11-05T09:02:00Z"))
            .apply_changes(
                &CalendarEventChanges {
                    ends_at: Some(instant("2026-11-05T09:01:00Z")),
                    ..CalendarEventChanges::default()
                },
                instant(NOW),
            );

        assert!(short_run.is_ok());
    }

    #[test]
    fn checks_planned_titles() {
        assert_eq!(validate_planned_title("  Gym ").expect("valid"), "Gym");
        assert!(validate_planned_title("   ").is_err());
        assert!(validate_planned_title(&"a".repeat(201)).is_err());
        assert!(validate_planned_title(&"é".repeat(200)).is_ok());
    }

    #[test]
    fn a_tracked_event_has_no_zone_or_repeat_of_its_own() {
        let finished_event =
            tracked_event(Some("2026-11-05T09:40:00Z"), Some("2026-11-05T10:00:00Z"));
        let utc_zone = parse_event_zone("UTC", "inputZone").expect("a known zone");

        for refused_changes in [
            CalendarEventChanges {
                time_zone_change: Some(Some(String::from("Europe/London"))),
                ..CalendarEventChanges::default()
            },
            CalendarEventChanges {
                repeat: Some(EventRepeat {
                    repeat_rule: serde_json::from_str(
                        r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
                    )
                    .expect("a valid rule"),
                    input_zone: utc_zone,
                }),
                ..CalendarEventChanges::default()
            },
        ] {
            let refused = finished_event.apply_changes(&refused_changes, instant(NOW));
            assert!(matches!(refused, Err(CalendarEventError::Invalid(_))));
        }
    }

    #[test]
    fn reads_a_local_time_in_its_input_zone() {
        let manila = parse_event_zone("Asia/Manila", "inputZone").expect("a known zone");

        assert_eq!(
            parse_event_moment("2026-11-09T18:00", "start", &manila).ok(),
            Some(instant("2026-11-09T10:00:00Z"))
        );
        for refused_text in ["2026-11-09T18:00:00Z", "2026-11-09", "tomorrow"] {
            assert!(parse_event_moment(refused_text, "start", &manila).is_err());
        }
        assert!(parse_event_zone("Mars/Olympus", "inputZone").is_err());
    }

    #[test]
    fn takes_whole_days_from_midnight_in_the_range_zone() {
        let london = parse_event_zone("Europe/London", "timeZone").expect("a known zone");

        assert_eq!(
            validate_event_range("2026-10-25", "1", &london).ok(),
            Some(EventRange {
                from: instant("2026-10-24T23:00:00Z"),
                to: instant("2026-10-26T00:00:00Z"),
            })
        );
        assert_eq!(
            validate_event_range("2026-11-02", "7", &london).ok(),
            Some(EventRange {
                from: instant("2026-11-02T00:00:00Z"),
                to: instant("2026-11-09T00:00:00Z"),
            })
        );
        for (start_text, day_count_text) in [
            ("2026-11-02", "3"),
            ("2026-11-02", ""),
            ("2026-02-30", "1"),
            ("2026-11-02T00:00:00Z", "1"),
        ] {
            assert!(validate_event_range(start_text, day_count_text, &london).is_err());
        }
    }
}
