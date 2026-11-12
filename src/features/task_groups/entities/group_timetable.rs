use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;

use crate::core::recurrence::{RepeatRule, latest_at_or_before, next_after, to_instant, to_local};
use crate::core::utc_instant::UtcInstant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowWindow {
    pub start: Time,
    pub end: Time,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowRule {
    pub repeat_rule: RepeatRule,
    pub start_date: Date,
    pub window: Option<ShowWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncheckRule {
    pub repeat_rule: RepeatRule,
    pub start_date: Date,
    pub time: Time,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupTimetable {
    pub is_due_now: bool,
    pub due_until: Option<UtcInstant>,
    pub next_shows_at: Option<UtcInstant>,
    pub last_uncheck_at: Option<UtcInstant>,
    pub next_uncheck_at: Option<UtcInstant>,
}

impl ShowRule {
    fn span_start_time(&self) -> Time {
        self.window
            .map_or(Time::midnight(), |show_window| show_window.start)
    }

    fn span_end_after(&self, span_start: UtcInstant, time_zone: &TimeZone) -> Option<UtcInstant> {
        let span_start_date = to_local(span_start, time_zone)?.date();
        let (end_date, end_time) = match self.window {
            Some(show_window) if show_window.end > show_window.start => {
                (span_start_date, show_window.end)
            }
            Some(show_window) => (span_start_date.tomorrow().ok()?, show_window.end),
            None => (span_start_date.tomorrow().ok()?, Time::midnight()),
        };
        to_instant(end_date.to_datetime(end_time), time_zone)
    }
}

impl GroupTimetable {
    pub fn work_out(
        show_rule: Option<&ShowRule>,
        uncheck_rule: Option<&UncheckRule>,
        time_zone: &TimeZone,
        now: UtcInstant,
    ) -> Self {
        let (is_due_now, due_until, next_shows_at) = match show_rule {
            None => (true, None, None),
            Some(show_rule) => {
                let show_start = show_rule
                    .start_date
                    .to_datetime(show_rule.span_start_time());
                let due_until =
                    latest_at_or_before(&show_rule.repeat_rule, show_start, time_zone, now)
                        .and_then(|span_start| show_rule.span_end_after(span_start, time_zone))
                        .filter(|span_end| *span_end > now);
                let next_shows_at = next_after(&show_rule.repeat_rule, show_start, time_zone, now);
                (due_until.is_some(), due_until, next_shows_at)
            }
        };
        let (last_uncheck_at, next_uncheck_at) =
            uncheck_rule.map_or((None, None), |uncheck_rule| {
                let uncheck_start = uncheck_rule.start_date.to_datetime(uncheck_rule.time);
                (
                    latest_at_or_before(&uncheck_rule.repeat_rule, uncheck_start, time_zone, now),
                    next_after(&uncheck_rule.repeat_rule, uncheck_start, time_zone, now),
                )
            });
        Self {
            is_due_now,
            due_until,
            next_shows_at,
            last_uncheck_at,
            next_uncheck_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};

    use super::*;
    use crate::core::recurrence::parse_time_zone;

    fn manila() -> TimeZone {
        parse_time_zone("Asia/Manila").expect("Manila should be a zone")
    }

    fn repeat_rule(rule_json: &str) -> RepeatRule {
        serde_json::from_str(rule_json).expect("the rule should parse")
    }

    fn every_day() -> RepeatRule {
        repeat_rule(r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#)
    }

    fn weekends() -> RepeatRule {
        repeat_rule(
            r#"{"frequency":"weekly","interval":1,"weekdays":["sat","sun"],"ends":{"never":true}}"#,
        )
    }

    fn weekdays() -> RepeatRule {
        repeat_rule(
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","tue","wed","thu","fri"],"ends":{"never":true}}"#,
        )
    }

    fn instant_at(local_text: &str, time_zone: &TimeZone) -> UtcInstant {
        to_instant(
            local_text.parse().expect("the local time should parse"),
            time_zone,
        )
        .expect("the local time should exist")
    }

    fn show_rule(repeat_rule: RepeatRule, window: Option<(Time, Time)>) -> ShowRule {
        ShowRule {
            repeat_rule,
            start_date: date(2026, 11, 2),
            window: window.map(|(start, end)| ShowWindow { start, end }),
        }
    }

    fn timetable_at(show_rule: &ShowRule, local_text: &str) -> GroupTimetable {
        GroupTimetable::work_out(
            Some(show_rule),
            None,
            &manila(),
            instant_at(local_text, &manila()),
        )
    }

    #[test]
    fn a_group_without_rules_is_always_due_with_no_times() {
        let group_timetable = GroupTimetable::work_out(None, None, &manila(), UtcInstant::now());

        assert_eq!(
            group_timetable,
            GroupTimetable {
                is_due_now: true,
                due_until: None,
                next_shows_at: None,
                last_uncheck_at: None,
                next_uncheck_at: None,
            }
        );
    }

    #[test]
    fn an_every_day_group_is_due_until_midnight() {
        let group_timetable = timetable_at(&show_rule(every_day(), None), "2026-11-12T15:00");

        assert!(group_timetable.is_due_now);
        assert_eq!(
            group_timetable.due_until,
            Some(instant_at("2026-11-13T00:00", &manila()))
        );
        assert_eq!(
            group_timetable.next_shows_at,
            Some(instant_at("2026-11-13T00:00", &manila()))
        );
    }

    #[test]
    fn a_weekend_group_is_not_due_on_a_thursday_and_next_shows_on_saturday() {
        let group_timetable = timetable_at(&show_rule(weekends(), None), "2026-11-12T15:00");

        assert!(!group_timetable.is_due_now);
        assert_eq!(group_timetable.due_until, None);
        assert_eq!(
            group_timetable.next_shows_at,
            Some(instant_at("2026-11-14T00:00", &manila()))
        );
    }

    #[test]
    fn a_morning_window_is_due_inside_it_and_not_after_it() {
        let morning_rule = show_rule(every_day(), Some((time(6, 0, 0, 0), time(12, 0, 0, 0))));

        let inside_timetable = timetable_at(&morning_rule, "2026-11-12T09:00");
        let after_timetable = timetable_at(&morning_rule, "2026-11-12T12:00");

        assert!(inside_timetable.is_due_now);
        assert_eq!(
            inside_timetable.due_until,
            Some(instant_at("2026-11-12T12:00", &manila()))
        );
        assert!(!after_timetable.is_due_now);
        assert_eq!(
            after_timetable.next_shows_at,
            Some(instant_at("2026-11-13T06:00", &manila()))
        );
    }

    #[test]
    fn a_window_across_midnight_belongs_to_the_day_it_starts() {
        let night_rule = show_rule(weekdays(), Some((time(22, 0, 0, 0), time(2, 0, 0, 0))));

        let friday_night_timetable = timetable_at(&night_rule, "2026-11-14T01:00");
        let saturday_night_timetable = timetable_at(&night_rule, "2026-11-15T01:00");

        assert!(friday_night_timetable.is_due_now);
        assert_eq!(
            friday_night_timetable.due_until,
            Some(instant_at("2026-11-14T02:00", &manila()))
        );
        assert!(!saturday_night_timetable.is_due_now);
        assert_eq!(
            saturday_night_timetable.next_shows_at,
            Some(instant_at("2026-11-16T22:00", &manila()))
        );
    }

    #[test]
    fn a_window_keeps_its_wall_clock_across_a_daylight_saving_change() {
        let london = parse_time_zone("Europe/London").expect("London should be a zone");
        let morning_rule = show_rule(every_day(), Some((time(6, 0, 0, 0), time(12, 0, 0, 0))));

        let group_timetable = GroupTimetable::work_out(
            Some(&morning_rule),
            None,
            &london,
            instant_at("2027-03-28T07:00", &london),
        );

        assert!(group_timetable.is_due_now);
        assert_eq!(
            group_timetable.due_until,
            Some(instant_at("2027-03-28T12:00", &london))
        );
    }

    #[test]
    fn an_ended_show_rule_is_never_due_again() {
        let ended_rule = show_rule(
            repeat_rule(r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2026-11-05"}}"#),
            None,
        );

        let group_timetable = timetable_at(&ended_rule, "2026-11-12T09:00");

        assert!(!group_timetable.is_due_now);
        assert_eq!(group_timetable.next_shows_at, None);
    }

    #[test]
    fn weekend_unchecks_at_four_are_worked_out_around_the_weekend() {
        let weekend_uncheck = UncheckRule {
            repeat_rule: weekends(),
            start_date: date(2026, 11, 2),
            time: time(4, 0, 0, 0),
        };
        let uncheck_times_at = |local_text: &str| {
            let group_timetable = GroupTimetable::work_out(
                None,
                Some(&weekend_uncheck),
                &manila(),
                instant_at(local_text, &manila()),
            );
            (
                group_timetable.last_uncheck_at,
                group_timetable.next_uncheck_at,
            )
        };

        assert_eq!(
            uncheck_times_at("2026-11-13T10:00"),
            (
                Some(instant_at("2026-11-08T04:00", &manila())),
                Some(instant_at("2026-11-14T04:00", &manila()))
            )
        );
        assert_eq!(
            uncheck_times_at("2026-11-14T03:59"),
            (
                Some(instant_at("2026-11-08T04:00", &manila())),
                Some(instant_at("2026-11-14T04:00", &manila()))
            )
        );
        assert_eq!(
            uncheck_times_at("2026-11-14T04:00"),
            (
                Some(instant_at("2026-11-14T04:00", &manila())),
                Some(instant_at("2026-11-15T04:00", &manila()))
            )
        );
        assert_eq!(
            uncheck_times_at("2026-11-16T09:00"),
            (
                Some(instant_at("2026-11-15T04:00", &manila())),
                Some(instant_at("2026-11-21T04:00", &manila()))
            )
        );
    }

    #[test]
    fn an_ended_uncheck_rule_stops_unchecking() {
        let ended_uncheck = UncheckRule {
            repeat_rule: repeat_rule(
                r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2026-11-05"}}"#,
            ),
            start_date: date(2026, 11, 2),
            time: time(0, 0, 0, 0),
        };

        let group_timetable = GroupTimetable::work_out(
            None,
            Some(&ended_uncheck),
            &manila(),
            instant_at("2026-11-12T09:00", &manila()),
        );

        assert_eq!(
            group_timetable.last_uncheck_at,
            Some(instant_at("2026-11-05T00:00", &manila()))
        );
        assert_eq!(group_timetable.next_uncheck_at, None);
    }
}
