use std::fmt;

use jiff::civil::{Date, DateTime, Weekday};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

use crate::core::utc_instant::UtcInstant;

const MAX_INTERVAL: i64 = 99;
const MAX_OCCURRENCE_COUNT: i64 = 999;
const LATEST_LOOKBACK_PERIODS: i64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatFrequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatWeekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl RepeatWeekday {
    const WEEK_ORDER: [Self; 7] = [
        Self::Mon,
        Self::Tue,
        Self::Wed,
        Self::Thu,
        Self::Fri,
        Self::Sat,
        Self::Sun,
    ];

    fn shifted_by_days(self, day_count: i64) -> Self {
        let week_index = Self::WEEK_ORDER
            .iter()
            .position(|week_weekday| *week_weekday == self)
            .unwrap_or_default();
        let shifted_index =
            (i64::try_from(week_index).unwrap_or_default() + day_count).rem_euclid(7);
        Self::WEEK_ORDER[usize::try_from(shifted_index).unwrap_or_default()]
    }

    fn to_weekday(self) -> Weekday {
        match self {
            Self::Mon => Weekday::Monday,
            Self::Tue => Weekday::Tuesday,
            Self::Wed => Weekday::Wednesday,
            Self::Thu => Weekday::Thursday,
            Self::Fri => Weekday::Friday,
            Self::Sat => Weekday::Saturday,
            Self::Sun => Weekday::Sunday,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonthlyRepeat {
    OnDay(i8),
    OnWeekday { week: i8, weekday: RepeatWeekday },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatEnd {
    Never,
    OnDate(Date),
    After(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RepeatRuleShape", into = "RepeatRuleShape")]
pub struct RepeatRule {
    frequency: RepeatFrequency,
    interval: i64,
    weekdays: Vec<RepeatWeekday>,
    monthly: Option<MonthlyRepeat>,
    ends: RepeatEnd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRepeatRule(&'static str);

impl fmt::Display for InvalidRepeatRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for InvalidRepeatRule {}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepeatRuleShape {
    frequency: RepeatFrequency,
    interval: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    weekdays: Option<Vec<RepeatWeekday>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    monthly: Option<MonthlyShape>,
    ends: EndsShape,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum MonthlyShape {
    OnDay(MonthlyDayShape),
    OnWeekday(MonthlyWeekdayShape),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MonthlyDayShape {
    day: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MonthlyWeekdayShape {
    week: i64,
    weekday: RepeatWeekday,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
enum EndsShape {
    Never(bool),
    OnDate(String),
    After(i64),
}

impl TryFrom<RepeatRuleShape> for RepeatRule {
    type Error = InvalidRepeatRule;

    fn try_from(rule_shape: RepeatRuleShape) -> Result<Self, Self::Error> {
        if !(1..=MAX_INTERVAL).contains(&rule_shape.interval) {
            return Err(InvalidRepeatRule("interval must be 1 to 99"));
        }
        let is_weekly = rule_shape.frequency == RepeatFrequency::Weekly;
        let weekdays = match (is_weekly, rule_shape.weekdays) {
            (true, Some(listed_weekdays)) => {
                let mut sorted_weekdays = listed_weekdays.clone();
                sorted_weekdays.sort();
                sorted_weekdays.dedup();
                if sorted_weekdays.is_empty() || sorted_weekdays.len() != listed_weekdays.len() {
                    return Err(InvalidRepeatRule("weekdays must be 1 to 7 different days"));
                }
                sorted_weekdays
            }
            (true, None) => return Err(InvalidRepeatRule("a weekly repeat needs weekdays")),
            (false, Some(_)) => {
                return Err(InvalidRepeatRule("only a weekly repeat has weekdays"));
            }
            (false, None) => Vec::new(),
        };
        let is_monthly = rule_shape.frequency == RepeatFrequency::Monthly;
        let monthly = match (is_monthly, rule_shape.monthly) {
            (true, Some(MonthlyShape::OnDay(day_shape))) => {
                let month_day = i8::try_from(day_shape.day)
                    .ok()
                    .filter(|month_day| (1..=31).contains(month_day))
                    .ok_or(InvalidRepeatRule("a monthly day must be 1 to 31"))?;
                Some(MonthlyRepeat::OnDay(month_day))
            }
            (true, Some(MonthlyShape::OnWeekday(weekday_shape))) => {
                let week = i8::try_from(weekday_shape.week)
                    .ok()
                    .filter(|week| (1..=4).contains(week) || *week == -1)
                    .ok_or(InvalidRepeatRule(
                        "a monthly week must be 1 to 4, or -1 for last",
                    ))?;
                Some(MonthlyRepeat::OnWeekday {
                    week,
                    weekday: weekday_shape.weekday,
                })
            }
            (true, None) => return Err(InvalidRepeatRule("a monthly repeat needs a day")),
            (false, Some(_)) => {
                return Err(InvalidRepeatRule("only a monthly repeat has a monthly day"));
            }
            (false, None) => None,
        };
        let ends = match rule_shape.ends {
            EndsShape::Never(true) => RepeatEnd::Never,
            EndsShape::Never(false) => return Err(InvalidRepeatRule("never must be true")),
            EndsShape::OnDate(end_date_text) => RepeatEnd::OnDate(
                parse_calendar_date(&end_date_text)
                    .ok_or(InvalidRepeatRule("the end date must be YYYY-MM-DD"))?,
            ),
            EndsShape::After(occurrence_count) => {
                if !(1..=MAX_OCCURRENCE_COUNT).contains(&occurrence_count) {
                    return Err(InvalidRepeatRule("a repeat ends after 1 to 999 times"));
                }
                RepeatEnd::After(occurrence_count)
            }
        };
        Ok(Self {
            frequency: rule_shape.frequency,
            interval: rule_shape.interval,
            weekdays,
            monthly,
            ends,
        })
    }
}

impl From<RepeatRule> for RepeatRuleShape {
    fn from(repeat_rule: RepeatRule) -> Self {
        Self {
            frequency: repeat_rule.frequency,
            interval: repeat_rule.interval,
            weekdays: (repeat_rule.frequency == RepeatFrequency::Weekly)
                .then_some(repeat_rule.weekdays),
            monthly: repeat_rule
                .monthly
                .map(|monthly_repeat| match monthly_repeat {
                    MonthlyRepeat::OnDay(month_day) => MonthlyShape::OnDay(MonthlyDayShape {
                        day: i64::from(month_day),
                    }),
                    MonthlyRepeat::OnWeekday { week, weekday } => {
                        MonthlyShape::OnWeekday(MonthlyWeekdayShape {
                            week: i64::from(week),
                            weekday,
                        })
                    }
                }),
            ends: match repeat_rule.ends {
                RepeatEnd::Never => EndsShape::Never(true),
                RepeatEnd::OnDate(end_date) => EndsShape::OnDate(end_date.to_string()),
                RepeatEnd::After(occurrence_count) => EndsShape::After(occurrence_count),
            },
        }
    }
}

impl RepeatRule {
    pub fn shifted_by_days(&self, day_count: i64) -> Self {
        let mut shifted_weekdays: Vec<RepeatWeekday> = self
            .weekdays
            .iter()
            .map(|repeat_weekday| repeat_weekday.shifted_by_days(day_count))
            .collect();
        shifted_weekdays.sort();
        let shifted_monthly = self.monthly.map(|monthly_repeat| match monthly_repeat {
            MonthlyRepeat::OnDay(month_day) => {
                let shifted_day = i64::from(month_day) + day_count;
                i8::try_from(shifted_day)
                    .ok()
                    .filter(|shifted_day| (1..=31).contains(shifted_day))
                    .map_or(monthly_repeat, MonthlyRepeat::OnDay)
            }
            MonthlyRepeat::OnWeekday { week, weekday } => MonthlyRepeat::OnWeekday {
                week,
                weekday: weekday.shifted_by_days(day_count),
            },
        });
        Self {
            weekdays: shifted_weekdays,
            monthly: shifted_monthly,
            ..self.clone()
        }
    }

    pub fn after_used_occurrences(&self, used_count: i64) -> Self {
        let ends = match self.ends {
            RepeatEnd::After(occurrence_count) => {
                RepeatEnd::After((occurrence_count - used_count).max(1))
            }
            other_end => other_end,
        };
        Self {
            ends,
            ..self.clone()
        }
    }
}

pub fn parse_calendar_date(date_text: &str) -> Option<Date> {
    let is_plain_date = date_text.len() == 10
        && date_text
            .bytes()
            .enumerate()
            .all(|(byte_index, date_byte)| match byte_index {
                4 | 7 => date_byte == b'-',
                _ => date_byte.is_ascii_digit(),
            });
    is_plain_date.then(|| date_text.parse().ok()).flatten()
}

pub fn parse_local_moment(moment_text: &str) -> Option<DateTime> {
    let is_plain_moment = matches!(moment_text.len(), 16 | 19)
        && moment_text
            .bytes()
            .enumerate()
            .all(|(byte_index, moment_byte)| match byte_index {
                4 | 7 => moment_byte == b'-',
                10 => moment_byte == b'T',
                13 | 16 => moment_byte == b':',
                _ => moment_byte.is_ascii_digit(),
            });
    is_plain_moment.then(|| moment_text.parse().ok()).flatten()
}

pub fn parse_time_zone(time_zone_name: &str) -> Option<TimeZone> {
    TimeZone::get(time_zone_name)
        .ok()
        .filter(|time_zone| time_zone.iana_name().is_some())
}

pub fn to_instant(local_moment: DateTime, time_zone: &TimeZone) -> Option<UtcInstant> {
    let moment_timestamp = time_zone
        .to_ambiguous_timestamp(local_moment)
        .compatible()
        .ok()?;
    Some(UtcInstant::from_unix_millis(
        moment_timestamp.as_millisecond(),
    ))
}

pub fn to_local(instant: UtcInstant, time_zone: &TimeZone) -> Option<DateTime> {
    let instant_timestamp = Timestamp::from_millisecond(instant.unix_millis()).ok()?;
    Some(time_zone.to_datetime(instant_timestamp))
}

pub fn occurrences(
    repeat_rule: &RepeatRule,
    local_start: DateTime,
    time_zone: &TimeZone,
    range_start: UtcInstant,
    range_end: UtcInstant,
) -> Vec<UtcInstant> {
    let (Some(range_start_local), Some(range_end_local)) = (
        to_local(range_start, time_zone),
        to_local(range_end, time_zone),
    ) else {
        return Vec::new();
    };
    let first_period_index =
        first_period_index(repeat_rule, local_start.date(), range_start_local.date(), 1);
    OccurrenceWalk::new(
        repeat_rule,
        local_start.date(),
        first_period_index,
        add_days(range_end_local.date(), 1),
    )
    .filter_map(|occurrence_date| {
        to_instant(occurrence_date.to_datetime(local_start.time()), time_zone)
    })
    .filter(|occurrence_instant| *occurrence_instant >= range_start)
    .take_while(|occurrence_instant| *occurrence_instant < range_end)
    .collect()
}

pub fn next_after(
    repeat_rule: &RepeatRule,
    local_start: DateTime,
    time_zone: &TimeZone,
    instant: UtcInstant,
) -> Option<UtcInstant> {
    let instant_local = to_local(instant, time_zone)?;
    let first_period_index =
        first_period_index(repeat_rule, local_start.date(), instant_local.date(), 1);
    OccurrenceWalk::new(repeat_rule, local_start.date(), first_period_index, None)
        .filter_map(|occurrence_date| {
            to_instant(occurrence_date.to_datetime(local_start.time()), time_zone)
        })
        .find(|occurrence_instant| *occurrence_instant > instant)
}

pub fn latest_at_or_before(
    repeat_rule: &RepeatRule,
    local_start: DateTime,
    time_zone: &TimeZone,
    instant: UtcInstant,
) -> Option<UtcInstant> {
    let instant_local = to_local(instant, time_zone)?;
    let last_walk_date = add_days(instant_local.date(), 1);
    let latest_from = |first_period_index| {
        OccurrenceWalk::new(
            repeat_rule,
            local_start.date(),
            first_period_index,
            last_walk_date,
        )
        .filter_map(|occurrence_date| {
            to_instant(occurrence_date.to_datetime(local_start.time()), time_zone)
        })
        .take_while(|occurrence_instant| *occurrence_instant <= instant)
        .last()
    };
    let lookback_period_index = first_period_index(
        repeat_rule,
        local_start.date(),
        instant_local.date(),
        LATEST_LOOKBACK_PERIODS,
    );
    latest_from(lookback_period_index).or_else(|| {
        (lookback_period_index > 0)
            .then(|| latest_from(0))
            .flatten()
    })
}

struct OccurrenceWalk<'rule> {
    repeat_rule: &'rule RepeatRule,
    anchor_date: Date,
    period_index: i64,
    last_walk_date: Option<Date>,
    period_dates: Vec<Date>,
    period_position: usize,
    remaining_count: Option<i64>,
    is_finished: bool,
}

impl<'rule> OccurrenceWalk<'rule> {
    fn new(
        repeat_rule: &'rule RepeatRule,
        anchor_date: Date,
        first_period_index: i64,
        last_walk_date: Option<Date>,
    ) -> Self {
        let end_date = match repeat_rule.ends {
            RepeatEnd::OnDate(end_date) => Some(end_date),
            RepeatEnd::Never | RepeatEnd::After(_) => None,
        };
        let remaining_count = match repeat_rule.ends {
            RepeatEnd::After(occurrence_count) => Some(occurrence_count),
            RepeatEnd::Never | RepeatEnd::OnDate(_) => None,
        };
        Self {
            repeat_rule,
            anchor_date,
            period_index: first_period_index,
            last_walk_date: match (end_date, last_walk_date) {
                (Some(end_date), Some(last_walk_date)) => Some(end_date.min(last_walk_date)),
                (end_date, last_walk_date) => end_date.or(last_walk_date),
            },
            period_dates: Vec::new(),
            period_position: 0,
            remaining_count,
            is_finished: false,
        }
    }
}

impl Iterator for OccurrenceWalk<'_> {
    type Item = Date;

    fn next(&mut self) -> Option<Date> {
        loop {
            if self.is_finished || self.remaining_count == Some(0) {
                return None;
            }
            if let Some(&period_date) = self.period_dates.get(self.period_position) {
                self.period_position += 1;
                if period_date < self.anchor_date {
                    continue;
                }
                if self
                    .last_walk_date
                    .is_some_and(|last_walk_date| period_date > last_walk_date)
                {
                    self.is_finished = true;
                    return None;
                }
                if let Some(remaining_count) = self.remaining_count.as_mut() {
                    *remaining_count -= 1;
                }
                return Some(period_date);
            }
            match period_span(self.repeat_rule, self.anchor_date, self.period_index) {
                Some((period_start, _))
                    if self
                        .last_walk_date
                        .is_some_and(|last_walk_date| period_start > last_walk_date) =>
                {
                    self.is_finished = true;
                }
                Some((_, period_dates)) => {
                    self.period_dates = period_dates;
                    self.period_position = 0;
                    self.period_index += 1;
                }
                None => self.is_finished = true,
            }
        }
    }
}

fn first_period_index(
    repeat_rule: &RepeatRule,
    anchor_date: Date,
    near_date: Date,
    lookback_periods: i64,
) -> i64 {
    if matches!(repeat_rule.ends, RepeatEnd::After(_)) {
        return 0;
    }
    let elapsed_units = match repeat_rule.frequency {
        RepeatFrequency::Daily => days_between(anchor_date, near_date),
        RepeatFrequency::Weekly => match (monday_of(anchor_date), monday_of(near_date)) {
            (Some(anchor_monday), Some(near_monday)) => {
                days_between(anchor_monday, near_monday).div_euclid(7)
            }
            _ => 0,
        },
        RepeatFrequency::Monthly => {
            (i64::from(near_date.year()) - i64::from(anchor_date.year())) * 12
                + i64::from(near_date.month())
                - i64::from(anchor_date.month())
        }
        RepeatFrequency::Yearly => i64::from(near_date.year()) - i64::from(anchor_date.year()),
    };
    (elapsed_units.div_euclid(repeat_rule.interval) - lookback_periods).max(0)
}

fn period_span(
    repeat_rule: &RepeatRule,
    anchor_date: Date,
    period_index: i64,
) -> Option<(Date, Vec<Date>)> {
    let elapsed_units = period_index.checked_mul(repeat_rule.interval)?;
    match repeat_rule.frequency {
        RepeatFrequency::Daily => {
            let period_date = add_days(anchor_date, elapsed_units)?;
            Some((period_date, vec![period_date]))
        }
        RepeatFrequency::Weekly => {
            let week_start = add_days(monday_of(anchor_date)?, elapsed_units.checked_mul(7)?)?;
            let week_dates = repeat_rule
                .weekdays
                .iter()
                .map(|repeat_weekday| {
                    add_days(
                        week_start,
                        i64::from(repeat_weekday.to_weekday().to_monday_zero_offset()),
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            Some((week_start, week_dates))
        }
        RepeatFrequency::Monthly => {
            let month_start = anchor_date
                .first_of_month()
                .checked_add(Span::new().try_months(elapsed_units).ok()?)
                .ok()?;
            let month_date = match repeat_rule.monthly? {
                MonthlyRepeat::OnDay(month_day) => {
                    Date::new(month_start.year(), month_start.month(), month_day).ok()
                }
                MonthlyRepeat::OnWeekday { week, weekday } => month_start
                    .nth_weekday_of_month(week, weekday.to_weekday())
                    .ok(),
            };
            Some((month_start, month_date.into_iter().collect()))
        }
        RepeatFrequency::Yearly => {
            let year =
                i16::try_from(i64::from(anchor_date.year()).checked_add(elapsed_units)?).ok()?;
            let year_start = Date::new(year, 1, 1).ok()?;
            let year_date = Date::new(year, anchor_date.month(), anchor_date.day()).ok();
            Some((year_start, year_date.into_iter().collect()))
        }
    }
}

fn add_days(date: Date, day_count: i64) -> Option<Date> {
    date.checked_add(Span::new().try_days(day_count).ok()?).ok()
}

fn monday_of(date: Date) -> Option<Date> {
    add_days(date, -i64::from(date.weekday().to_monday_zero_offset()))
}

fn days_between(earlier_date: Date, later_date: Date) -> i64 {
    later_date
        .duration_since(earlier_date)
        .as_secs()
        .div_euclid(86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(rule_json: &str) -> RepeatRule {
        serde_json::from_str(rule_json).expect("a valid repeat rule")
    }

    fn zone(time_zone_name: &str) -> TimeZone {
        parse_time_zone(time_zone_name).expect("a known time zone")
    }

    fn local(local_text: &str) -> DateTime {
        local_text.parse().expect("a valid local date and time")
    }

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid instant")
    }

    fn occurrence_texts(
        rule_json: &str,
        local_start: &str,
        time_zone_name: &str,
        range_start: &str,
        range_end: &str,
    ) -> Vec<String> {
        occurrences(
            &rule(rule_json),
            local(local_start),
            &zone(time_zone_name),
            instant(range_start),
            instant(range_end),
        )
        .into_iter()
        .map(UtcInstant::to_iso_text)
        .collect()
    }

    const WEEKLY_ON_MONDAY: &str =
        r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#;

    #[test]
    fn reads_and_writes_every_picker_shape_unchanged() {
        for rule_json in [
            r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
            WEEKLY_ON_MONDAY,
            r#"{"frequency":"monthly","interval":1,"monthly":{"week":2,"weekday":"mon"},"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":3,"monthly":{"day":31},"ends":{"after":12}}"#,
            r#"{"frequency":"yearly","interval":1,"ends":{"onDate":"2030-11-09"}}"#,
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","tue","wed","thu","fri"],"ends":{"never":true}}"#,
        ] {
            let written_json = serde_json::to_string(&rule(rule_json)).expect("rule writes");
            assert_eq!(written_json, rule_json);
        }
    }

    #[test]
    fn keeps_weekdays_in_week_order() {
        let shuffled_rule = rule(
            r#"{"frequency":"weekly","interval":1,"weekdays":["fri","mon"],"ends":{"never":true}}"#,
        );

        assert_eq!(
            serde_json::to_string(&shuffled_rule).expect("rule writes"),
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","fri"],"ends":{"never":true}}"#
        );
    }

    #[test]
    fn refuses_rules_outside_the_picker() {
        for rejected_json in [
            r#"{"frequency":"daily","interval":0,"ends":{"never":true}}"#,
            r#"{"frequency":"daily","interval":100,"ends":{"never":true}}"#,
            r#"{"frequency":"hourly","interval":1,"ends":{"never":true}}"#,
            r#"{"frequency":"weekly","interval":1,"ends":{"never":true}}"#,
            r#"{"frequency":"weekly","interval":1,"weekdays":[],"ends":{"never":true}}"#,
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","mon"],"ends":{"never":true}}"#,
            r#"{"frequency":"daily","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"monthly":{"day":0},"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"monthly":{"day":32},"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"monthly":{"week":5,"weekday":"mon"},"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"monthly":{"week":-2,"weekday":"mon"},"ends":{"never":true}}"#,
            r#"{"frequency":"monthly","interval":1,"monthly":{"day":3,"week":1},"ends":{"never":true}}"#,
            r#"{"frequency":"yearly","interval":1,"monthly":{"day":3},"ends":{"never":true}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"never":false}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"after":0}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"after":1000}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"onDate":"2027-02-30"}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"onDate":"27-03-31"}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"never":true,"after":3}}"#,
            r#"{"frequency":"daily","interval":1,"ends":{"never":true},"color":"red"}"#,
        ] {
            assert!(
                serde_json::from_str::<RepeatRule>(rejected_json).is_err(),
                "{rejected_json} should be refused"
            );
        }
    }

    #[test]
    fn repeats_every_day() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
                "2026-11-09T07:00:00",
                "Asia/Manila",
                "2026-11-08T00:00:00Z",
                "2026-11-12T00:00:00Z",
            ),
            [
                "2026-11-08T23:00:00.000Z",
                "2026-11-09T23:00:00.000Z",
                "2026-11-10T23:00:00.000Z",
                "2026-11-11T23:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_weekly_on_the_chosen_weekday() {
        assert_eq!(
            occurrence_texts(
                WEEKLY_ON_MONDAY,
                "2026-11-09T18:00:00",
                "Asia/Manila",
                "2026-11-01T00:00:00Z",
                "2026-11-30T00:00:00Z",
            ),
            [
                "2026-11-09T10:00:00.000Z",
                "2026-11-16T10:00:00.000Z",
                "2026-11-23T10:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_monthly_on_the_nth_weekday() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"monthly","interval":1,"monthly":{"week":2,"weekday":"mon"},"ends":{"never":true}}"#,
                "2026-11-09T18:00:00",
                "Asia/Manila",
                "2026-11-01T00:00:00Z",
                "2027-02-01T00:00:00Z",
            ),
            [
                "2026-11-09T10:00:00.000Z",
                "2026-12-14T10:00:00.000Z",
                "2027-01-11T10:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_monthly_on_the_last_weekday() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"monthly","interval":1,"monthly":{"week":-1,"weekday":"fri"},"ends":{"never":true}}"#,
                "2026-11-27T09:00:00",
                "UTC",
                "2026-11-01T00:00:00Z",
                "2027-02-01T00:00:00Z",
            ),
            [
                "2026-11-27T09:00:00.000Z",
                "2026-12-25T09:00:00.000Z",
                "2027-01-29T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_annually_on_the_first_date() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"yearly","interval":1,"ends":{"never":true}}"#,
                "2026-11-09T09:00:00",
                "UTC",
                "2026-01-01T00:00:00Z",
                "2029-01-01T00:00:00Z",
            ),
            [
                "2026-11-09T09:00:00.000Z",
                "2027-11-09T09:00:00.000Z",
                "2028-11-09T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_every_weekday() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon","tue","wed","thu","fri"],"ends":{"never":true}}"#,
                "2026-11-09T09:00:00",
                "UTC",
                "2026-11-09T00:00:00Z",
                "2026-11-17T00:00:00Z",
            ),
            [
                "2026-11-09T09:00:00.000Z",
                "2026-11-10T09:00:00.000Z",
                "2026-11-11T09:00:00.000Z",
                "2026-11-12T09:00:00.000Z",
                "2026-11-13T09:00:00.000Z",
                "2026-11-16T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn skips_whole_intervals() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"daily","interval":3,"ends":{"never":true}}"#,
                "2026-11-09T09:00:00",
                "UTC",
                "2026-11-09T00:00:00Z",
                "2026-11-17T00:00:00Z",
            ),
            [
                "2026-11-09T09:00:00.000Z",
                "2026-11-12T09:00:00.000Z",
                "2026-11-15T09:00:00.000Z",
            ]
        );
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"weekly","interval":2,"weekdays":["thu","mon"],"ends":{"never":true}}"#,
                "2026-11-12T09:00:00",
                "UTC",
                "2026-11-01T00:00:00Z",
                "2026-12-12T00:00:00Z",
            ),
            [
                "2026-11-12T09:00:00.000Z",
                "2026-11-23T09:00:00.000Z",
                "2026-11-26T09:00:00.000Z",
                "2026-12-07T09:00:00.000Z",
                "2026-12-10T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn skips_months_without_the_day() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"monthly","interval":1,"monthly":{"day":31},"ends":{"never":true}}"#,
                "2027-01-31T09:00:00",
                "UTC",
                "2027-01-01T00:00:00Z",
                "2027-09-01T00:00:00Z",
            ),
            [
                "2027-01-31T09:00:00.000Z",
                "2027-03-31T09:00:00.000Z",
                "2027-05-31T09:00:00.000Z",
                "2027-07-31T09:00:00.000Z",
                "2027-08-31T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn repeats_29_february_only_in_leap_years() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"yearly","interval":1,"ends":{"never":true}}"#,
                "2028-02-29T09:00:00",
                "UTC",
                "2028-01-01T00:00:00Z",
                "2037-01-01T00:00:00Z",
            ),
            [
                "2028-02-29T09:00:00.000Z",
                "2032-02-29T09:00:00.000Z",
                "2036-02-29T09:00:00.000Z",
            ]
        );
    }

    #[test]
    fn stops_on_the_end_date_including_it() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"onDate":"2026-11-23"}}"#,
                "2026-11-09T18:00:00",
                "Asia/Manila",
                "2026-11-01T00:00:00Z",
                "2027-01-01T00:00:00Z",
            ),
            [
                "2026-11-09T10:00:00.000Z",
                "2026-11-16T10:00:00.000Z",
                "2026-11-23T10:00:00.000Z",
            ]
        );
    }

    #[test]
    fn stops_after_the_chosen_number_of_times() {
        let after_three =
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","thu"],"ends":{"after":3}}"#;

        assert_eq!(
            occurrence_texts(
                after_three,
                "2026-11-09T09:00:00",
                "UTC",
                "2026-11-01T00:00:00Z",
                "2027-01-01T00:00:00Z",
            ),
            [
                "2026-11-09T09:00:00.000Z",
                "2026-11-12T09:00:00.000Z",
                "2026-11-16T09:00:00.000Z",
            ]
        );
        assert_eq!(
            occurrence_texts(
                after_three,
                "2026-11-09T09:00:00",
                "UTC",
                "2026-11-13T00:00:00Z",
                "2027-01-01T00:00:00Z",
            ),
            ["2026-11-16T09:00:00.000Z"]
        );
    }

    #[test]
    fn keeps_the_local_time_across_both_daylight_saving_changes() {
        let weekly_on_sunday =
            r#"{"frequency":"weekly","interval":1,"weekdays":["sun"],"ends":{"never":true}}"#;

        assert_eq!(
            occurrence_texts(
                weekly_on_sunday,
                "2026-03-22T19:00:00",
                "Europe/London",
                "2026-03-22T00:00:00Z",
                "2026-04-06T00:00:00Z",
            ),
            [
                "2026-03-22T19:00:00.000Z",
                "2026-03-29T18:00:00.000Z",
                "2026-04-05T18:00:00.000Z",
            ]
        );
        assert_eq!(
            occurrence_texts(
                weekly_on_sunday,
                "2026-03-22T19:00:00",
                "Europe/London",
                "2026-10-18T00:00:00Z",
                "2026-10-26T00:00:00Z",
            ),
            ["2026-10-18T18:00:00.000Z", "2026-10-25T19:00:00.000Z"]
        );
    }

    #[test]
    fn moves_a_missing_local_time_forward_and_takes_the_earlier_of_a_repeated_one() {
        let daily = r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#;

        assert_eq!(
            occurrence_texts(
                daily,
                "2026-03-28T01:30:00",
                "Europe/London",
                "2026-03-28T00:00:00Z",
                "2026-03-31T00:00:00Z",
            ),
            [
                "2026-03-28T01:30:00.000Z",
                "2026-03-29T01:30:00.000Z",
                "2026-03-30T00:30:00.000Z",
            ]
        );
        assert_eq!(
            occurrence_texts(
                daily,
                "2026-10-24T01:30:00",
                "Europe/London",
                "2026-10-24T00:00:00Z",
                "2026-10-27T00:00:00Z",
            ),
            [
                "2026-10-24T00:30:00.000Z",
                "2026-10-25T00:30:00.000Z",
                "2026-10-26T01:30:00.000Z",
            ]
        );
    }

    #[test]
    fn finds_far_future_occurrences_of_a_series_that_never_ends() {
        assert_eq!(
            occurrence_texts(
                r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#,
                "2026-11-09T07:00:00",
                "Asia/Manila",
                "2126-11-09T00:00:00Z",
                "2126-11-12T00:00:00Z",
            ),
            [
                "2126-11-09T23:00:00.000Z",
                "2126-11-10T23:00:00.000Z",
                "2126-11-11T23:00:00.000Z",
            ]
        );
    }

    #[test]
    fn finds_nothing_before_the_series_starts() {
        assert!(
            occurrence_texts(
                WEEKLY_ON_MONDAY,
                "2026-11-09T18:00:00",
                "Asia/Manila",
                "2026-10-01T00:00:00Z",
                "2026-11-09T10:00:00Z",
            )
            .is_empty()
        );
    }

    #[test]
    fn finds_the_next_occurrence_after_a_moment() {
        let weekly_rule = rule(WEEKLY_ON_MONDAY);
        let manila = zone("Asia/Manila");
        let start = local("2026-11-09T18:00:00");
        let next_text = |after_text| {
            next_after(&weekly_rule, start, &manila, instant(after_text))
                .map(UtcInstant::to_iso_text)
        };

        assert_eq!(
            next_text("2026-10-01T00:00:00Z").as_deref(),
            Some("2026-11-09T10:00:00.000Z")
        );
        assert_eq!(
            next_text("2026-11-10T00:00:00Z").as_deref(),
            Some("2026-11-16T10:00:00.000Z")
        );
        assert_eq!(
            next_text("2026-11-16T10:00:00Z").as_deref(),
            Some("2026-11-23T10:00:00.000Z")
        );
        assert_eq!(
            next_text("2126-11-10T00:00:00Z").as_deref(),
            Some("2126-11-11T10:00:00.000Z")
        );
    }

    #[test]
    fn finds_no_next_occurrence_once_the_series_has_ended() {
        let twice = rule(r#"{"frequency":"daily","interval":1,"ends":{"after":2}}"#);

        assert_eq!(
            next_after(
                &twice,
                local("2026-11-09T09:00:00"),
                &zone("UTC"),
                instant("2026-11-10T09:00:00Z"),
            ),
            None
        );
    }

    #[test]
    fn finds_the_latest_occurrence_at_or_before_a_moment() {
        let weekly_rule = rule(WEEKLY_ON_MONDAY);
        let manila = zone("Asia/Manila");
        let start = local("2026-11-09T18:00:00");
        let latest_text = |before_text| {
            latest_at_or_before(&weekly_rule, start, &manila, instant(before_text))
                .map(UtcInstant::to_iso_text)
        };

        assert_eq!(latest_text("2026-11-09T09:59:59Z"), None);
        assert_eq!(
            latest_text("2026-11-09T10:00:00Z").as_deref(),
            Some("2026-11-09T10:00:00.000Z")
        );
        assert_eq!(
            latest_text("2026-12-02T00:00:00Z").as_deref(),
            Some("2026-11-30T10:00:00.000Z")
        );
        assert_eq!(
            latest_text("2126-11-17T00:00:00Z").as_deref(),
            Some("2126-11-11T10:00:00.000Z")
        );
    }

    #[test]
    fn finds_the_last_occurrence_of_an_ended_series_as_the_latest() {
        let leap_days = rule(r#"{"frequency":"yearly","interval":1,"ends":{"after":2}}"#);

        assert_eq!(
            latest_at_or_before(
                &leap_days,
                local("2028-02-29T09:00:00"),
                &zone("UTC"),
                instant("2040-01-01T00:00:00Z"),
            )
            .map(UtcInstant::to_iso_text)
            .as_deref(),
            Some("2032-02-29T09:00:00.000Z")
        );
        assert_eq!(
            latest_at_or_before(
                &rule(r#"{"frequency":"monthly","interval":1,"monthly":{"day":31},"ends":{"never":true}}"#),
                local("2027-01-31T09:00:00"),
                &zone("UTC"),
                instant("2027-03-30T00:00:00Z"),
            )
            .map(UtcInstant::to_iso_text)
            .as_deref(),
            Some("2027-01-31T09:00:00.000Z")
        );
    }

    #[test]
    fn converts_between_local_times_and_instants() {
        let london = zone("Europe/London");

        assert_eq!(
            to_instant(local("2026-07-01T09:00:00"), &london).map(UtcInstant::to_iso_text),
            Some("2026-07-01T08:00:00.000Z".to_string())
        );
        assert_eq!(
            to_instant(local("2026-03-29T01:30:00"), &london).map(UtcInstant::to_iso_text),
            Some("2026-03-29T01:30:00.000Z".to_string())
        );
        assert_eq!(
            to_local(instant("2026-07-01T08:00:00Z"), &london),
            Some(local("2026-07-01T09:00:00"))
        );
    }

    #[test]
    fn moves_the_repeat_days_along_with_a_shifted_occurrence() {
        let shifted_json = |rule_json: &str, day_count| {
            serde_json::to_string(&rule(rule_json).shifted_by_days(day_count)).expect("rule writes")
        };

        assert_eq!(
            shifted_json(
                r#"{"frequency":"weekly","interval":1,"weekdays":["mon","sun"],"ends":{"never":true}}"#,
                1,
            ),
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon","tue"],"ends":{"never":true}}"#
        );
        assert_eq!(
            shifted_json(
                r#"{"frequency":"monthly","interval":1,"monthly":{"week":-1,"weekday":"mon"},"ends":{"never":true}}"#,
                -2,
            ),
            r#"{"frequency":"monthly","interval":1,"monthly":{"week":-1,"weekday":"sat"},"ends":{"never":true}}"#
        );
        assert_eq!(
            shifted_json(
                r#"{"frequency":"monthly","interval":1,"monthly":{"day":15},"ends":{"never":true}}"#,
                3,
            ),
            r#"{"frequency":"monthly","interval":1,"monthly":{"day":18},"ends":{"never":true}}"#
        );
        assert_eq!(
            shifted_json(
                r#"{"frequency":"monthly","interval":1,"monthly":{"day":30},"ends":{"never":true}}"#,
                3,
            ),
            r#"{"frequency":"monthly","interval":1,"monthly":{"day":30},"ends":{"never":true}}"#
        );
        assert_eq!(
            shifted_json(
                r#"{"frequency":"daily","interval":2,"ends":{"after":5}}"#,
                3
            ),
            r#"{"frequency":"daily","interval":2,"ends":{"after":5}}"#
        );
    }

    #[test]
    fn counts_only_the_occurrences_left_after_a_split() {
        let written_json = |rule_json: &str| {
            serde_json::to_string(&rule(rule_json).after_used_occurrences(2)).expect("rule writes")
        };

        assert_eq!(
            written_json(r#"{"frequency":"daily","interval":1,"ends":{"after":5}}"#),
            r#"{"frequency":"daily","interval":1,"ends":{"after":3}}"#
        );
        assert_eq!(
            written_json(r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#),
            r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#
        );
    }

    #[test]
    fn reads_only_plain_local_times() {
        assert_eq!(
            parse_local_moment("2026-11-09T18:00"),
            Some(local("2026-11-09T18:00:00"))
        );
        assert_eq!(
            parse_local_moment("2026-11-09T18:00:30"),
            Some(local("2026-11-09T18:00:30"))
        );
        for refused_text in [
            "",
            "2026-11-09",
            "2026-11-09T18",
            "2026-11-09T18:00Z",
            "2026-11-09T18:00:00Z",
            "2026-11-09T18:00+08:00",
            "2026-11-09 18:00",
            "2026-11-09T18:00:00.5",
            "2026-02-30T18:00",
            "2026-11-09T24:00",
            "2026-11-09T18:60",
        ] {
            assert_eq!(parse_local_moment(refused_text), None, "{refused_text}");
        }
    }

    #[test]
    fn reads_only_plain_calendar_dates() {
        assert_eq!(
            parse_calendar_date("2028-02-29"),
            Some("2028-02-29".parse().expect("a date"))
        );
        for refused_text in ["2027-02-29", "2026-11-9", "2026-11-09T00:00", "+2026-11-09"] {
            assert_eq!(parse_calendar_date(refused_text), None, "{refused_text}");
        }
    }

    #[test]
    fn accepts_only_iana_time_zone_names() {
        for known_name in [
            "Europe/London",
            "Asia/Manila",
            "America/Argentina/Buenos_Aires",
            "UTC",
        ] {
            assert!(parse_time_zone(known_name).is_some(), "{known_name}");
        }
        for unknown_name in ["", "Mars/Olympus", "+08:00", "GMT+8 ", "Local"] {
            assert!(parse_time_zone(unknown_name).is_none(), "{unknown_name}");
        }
    }
}
