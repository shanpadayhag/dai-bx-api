use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use uuid::Uuid;

use crate::core::recurrence::{RepeatRule, to_local};
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::entities::group_timetable::{
    GroupTimetable, ShowRule, ShowWindow, UncheckRule,
};
use crate::features::task_groups::error::TaskGroupError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRules {
    pub show_rule: Option<ShowRule>,
    pub uncheck_rule: Option<UncheckRule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRulesListing {
    pub calendar_time_zone_name: Option<String>,
    pub group_rules: Vec<(Uuid, GroupRules)>,
}

impl GroupRulesListing {
    pub fn work_out_timetables(
        self,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Vec<(Uuid, GroupRules, GroupTimetable)> {
        let calendar_time_zone =
            request_time_zone.calendar_time_zone(self.calendar_time_zone_name.as_deref());
        self.group_rules
            .into_iter()
            .map(|(task_group_id, group_rules)| {
                let group_timetable = GroupTimetable::work_out(
                    group_rules.show_rule.as_ref(),
                    group_rules.uncheck_rule.as_ref(),
                    &calendar_time_zone,
                    now,
                );
                (task_group_id, group_rules, group_timetable)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowRuleInput {
    pub repeat_rule: RepeatRule,
    pub window: Option<ShowWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncheckRuleInput {
    pub repeat_rule: RepeatRule,
    pub time: Time,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRuleChanges {
    pub show_change: Option<Option<ShowRuleInput>>,
    pub uncheck_change: Option<Option<UncheckRuleInput>>,
}

pub fn today_in(time_zone: &TimeZone, now: UtcInstant) -> Result<Date, TaskGroupError> {
    to_local(now, time_zone)
        .map(|local_now| local_now.date())
        .ok_or_else(|| TaskGroupError::Invalid(String::from("today is out of range")))
}

pub fn parse_wall_clock_time(time_text: &str) -> Result<Time, TaskGroupError> {
    let is_hours_and_minutes = time_text.len() == 5 && time_text.as_bytes().get(2) == Some(&b':');
    is_hours_and_minutes
        .then(|| time_text.parse::<Time>().ok())
        .flatten()
        .ok_or_else(|| TaskGroupError::Invalid(String::from("a time is written as HH:MM")))
}

pub fn build_show_window(
    window_start_text: Option<&str>,
    window_end_text: Option<&str>,
) -> Result<Option<ShowWindow>, TaskGroupError> {
    match (window_start_text, window_end_text) {
        (None, None) => Ok(None),
        (Some(window_start_text), Some(window_end_text)) => {
            let show_window = ShowWindow {
                start: parse_wall_clock_time(window_start_text)?,
                end: parse_wall_clock_time(window_end_text)?,
            };
            if show_window.start == show_window.end {
                return Err(TaskGroupError::Invalid(String::from(
                    "a window starts and ends at different times",
                )));
            }
            Ok(Some(show_window))
        }
        _ => Err(TaskGroupError::Invalid(String::from(
            "a window needs both a start and an end",
        ))),
    }
}

impl GroupRuleChanges {
    pub fn apply_to(&self, current_rules: &GroupRules, today: Date) -> GroupRules {
        let show_rule = match &self.show_change {
            None => current_rules.show_rule.clone(),
            Some(None) => None,
            Some(Some(show_input)) => Some(ShowRule {
                start_date: current_rules
                    .show_rule
                    .as_ref()
                    .filter(|current_show| current_show.repeat_rule == show_input.repeat_rule)
                    .map_or(today, |current_show| current_show.start_date),
                repeat_rule: show_input.repeat_rule.clone(),
                window: show_input.window,
            }),
        };
        let uncheck_rule = match &self.uncheck_change {
            None => current_rules.uncheck_rule.clone(),
            Some(None) => None,
            Some(Some(uncheck_input)) => Some(UncheckRule {
                start_date: current_rules
                    .uncheck_rule
                    .as_ref()
                    .filter(|current_uncheck| {
                        current_uncheck.repeat_rule == uncheck_input.repeat_rule
                    })
                    .map_or(today, |current_uncheck| current_uncheck.start_date),
                repeat_rule: uncheck_input.repeat_rule.clone(),
                time: uncheck_input.time,
            }),
        };
        GroupRules {
            show_rule,
            uncheck_rule,
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};

    use super::*;

    fn every_day() -> RepeatRule {
        serde_json::from_str(r#"{"frequency":"daily","interval":1,"ends":{"never":true}}"#)
            .expect("the rule should parse")
    }

    fn weekends() -> RepeatRule {
        serde_json::from_str(
            r#"{"frequency":"weekly","interval":1,"weekdays":["sat","sun"],"ends":{"never":true}}"#,
        )
        .expect("the rule should parse")
    }

    fn rules_from(start_date: Date) -> GroupRules {
        GroupRules {
            show_rule: Some(ShowRule {
                repeat_rule: every_day(),
                start_date,
                window: None,
            }),
            uncheck_rule: Some(UncheckRule {
                repeat_rule: weekends(),
                start_date,
                time: time(4, 0, 0, 0),
            }),
        }
    }

    #[test]
    fn reads_hours_and_minutes_only() {
        assert_eq!(parse_wall_clock_time("04:00").ok(), Some(time(4, 0, 0, 0)));
        for refused_text in ["4:00", "24:00", "04:00:00", "noon", ""] {
            assert!(
                parse_wall_clock_time(refused_text).is_err(),
                "{refused_text}"
            );
        }
    }

    #[test]
    fn a_window_needs_both_ends_and_different_times() {
        assert_eq!(build_show_window(None, None).ok(), Some(None));
        assert!(build_show_window(Some("06:00"), None).is_err());
        assert!(build_show_window(Some("06:00"), Some("06:00")).is_err());
        assert_eq!(
            build_show_window(Some("22:00"), Some("02:00")).ok(),
            Some(Some(ShowWindow {
                start: time(22, 0, 0, 0),
                end: time(2, 0, 0, 0),
            }))
        );
    }

    #[test]
    fn missing_changes_keep_the_rules_and_null_clears_them() {
        let current_rules = rules_from(date(2026, 11, 2));

        assert_eq!(
            GroupRuleChanges::default().apply_to(&current_rules, date(2026, 11, 12)),
            current_rules
        );
        assert_eq!(
            GroupRuleChanges {
                show_change: Some(None),
                uncheck_change: Some(None),
            }
            .apply_to(&current_rules, date(2026, 11, 12)),
            GroupRules::default()
        );
    }

    #[test]
    fn a_new_repeat_counts_from_today_and_the_same_repeat_keeps_its_start() {
        let current_rules = rules_from(date(2026, 11, 2));

        let changed_rules = GroupRuleChanges {
            show_change: Some(Some(ShowRuleInput {
                repeat_rule: weekends(),
                window: None,
            })),
            uncheck_change: Some(Some(UncheckRuleInput {
                repeat_rule: weekends(),
                time: time(5, 0, 0, 0),
            })),
        }
        .apply_to(&current_rules, date(2026, 11, 12));

        let changed_show = changed_rules.show_rule.expect("a show rule");
        let changed_uncheck = changed_rules.uncheck_rule.expect("an uncheck rule");
        assert_eq!(changed_show.start_date, date(2026, 11, 12));
        assert_eq!(changed_uncheck.start_date, date(2026, 11, 2));
        assert_eq!(changed_uncheck.time, time(5, 0, 0, 0));
    }
}
