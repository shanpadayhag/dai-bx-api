use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::parse_time_zone;
use crate::core::wall_clock::{WallClockWindow, format_wall_clock_time, parse_wall_clock_time};
use crate::features::account::error::AccountError;

pub const PLANNING_WEEK_DAY_COUNT: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningDay {
    pub start: String,
    pub end: String,
}

impl PlanningDay {
    pub fn window(&self) -> Option<WallClockWindow> {
        WallClockWindow::new(
            parse_wall_clock_time(&self.start)?,
            parse_wall_clock_time(&self.end)?,
        )
    }
}

pub type PlanningWeek = Vec<Option<PlanningDay>>;

pub const CHECK_IN_MINUTES_RANGE: std::ops::RangeInclusive<i32> = 5..=240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeTrackingSettings {
    pub check_ins_enabled: bool,
    pub check_in_interval_minutes: i32,
    pub first_check_in_minutes: i32,
    pub working_hours_start: String,
    pub working_hours_end: String,
    pub tracked_category_id: Option<Uuid>,
    pub calendar_time_zone: Option<String>,
    pub planning_hours: Option<Json<PlanningWeek>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeTrackingChanges {
    pub check_ins_enabled: Option<bool>,
    pub check_in_interval_minutes: Option<i32>,
    pub first_check_in_minutes: Option<i32>,
    pub working_hours_start: Option<String>,
    pub working_hours_end: Option<String>,
    pub tracked_category_change: Option<Option<Uuid>>,
    pub calendar_time_zone_change: Option<Option<String>>,
    pub planning_hours_change: Option<Option<PlanningWeek>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeTrackingWrite {
    Saved(TimeTrackingSettings),
    CategoryMissing,
    AccountMissing,
}

pub fn validate_check_in_minutes(raw_minutes: i64) -> Result<i32, AccountError> {
    i32::try_from(raw_minutes)
        .ok()
        .filter(|check_in_minutes| CHECK_IN_MINUTES_RANGE.contains(check_in_minutes))
        .ok_or_else(|| AccountError::Invalid(String::from("check-ins are 5 to 240 minutes apart")))
}

pub fn validate_calendar_time_zone(raw_zone_name: &str) -> Result<String, AccountError> {
    parse_time_zone(raw_zone_name)
        .map(|_| raw_zone_name.to_owned())
        .ok_or_else(|| {
            AccountError::Invalid(String::from(
                "the calendar time zone is a name like Asia/Manila",
            ))
        })
}

pub fn validate_clock_time(raw_time: &str) -> Result<String, AccountError> {
    let invalid_time = || {
        AccountError::Invalid(String::from(
            "working hours are like 09:30, from 00:00 to 23:59",
        ))
    };
    let time_bytes = raw_time.as_bytes();
    if time_bytes.len() != 5
        || time_bytes[2] != b':'
        || !time_bytes
            .iter()
            .enumerate()
            .all(|(byte_index, time_byte)| byte_index == 2 || time_byte.is_ascii_digit())
    {
        return Err(invalid_time());
    }
    let hour = (time_bytes[0] - b'0') * 10 + (time_bytes[1] - b'0');
    let minute = (time_bytes[3] - b'0') * 10 + (time_bytes[4] - b'0');
    if hour > 23 || minute > 59 {
        return Err(invalid_time());
    }
    Ok(raw_time.to_owned())
}

pub fn validate_planning_day(
    start_text: &str,
    end_text: &str,
) -> Result<PlanningDay, AccountError> {
    let parse_time = |time_text: &str| {
        parse_wall_clock_time(time_text)
            .ok_or_else(|| AccountError::Invalid(String::from("a time is written as HH:MM")))
    };
    let planning_window = WallClockWindow::new(parse_time(start_text)?, parse_time(end_text)?)
        .ok_or_else(|| {
            AccountError::Invalid(String::from("a window starts and ends at different times"))
        })?;
    Ok(PlanningDay {
        start: format_wall_clock_time(planning_window.start),
        end: format_wall_clock_time(planning_window.end),
    })
}

pub fn validate_planning_week_length(day_count: usize) -> Result<(), AccountError> {
    if day_count == PLANNING_WEEK_DAY_COUNT {
        return Ok(());
    }
    Err(AccountError::Invalid(String::from(
        "planning hours list seven days, Monday first",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_check_ins_from_5_to_240_minutes() {
        assert_eq!(validate_check_in_minutes(5).ok(), Some(5));
        assert_eq!(validate_check_in_minutes(240).ok(), Some(240));
        for refused_minutes in [i64::MIN, -5, 0, 4, 241, i64::MAX] {
            assert!(
                validate_check_in_minutes(refused_minutes).is_err(),
                "{refused_minutes}"
            );
        }
    }

    #[test]
    fn accepts_only_named_calendar_time_zones() {
        assert_eq!(
            validate_calendar_time_zone("Europe/London").ok().as_deref(),
            Some("Europe/London")
        );
        for refused_zone_name in ["", "+08:00", "Mars/Olympus"] {
            assert!(
                validate_calendar_time_zone(refused_zone_name).is_err(),
                "{refused_zone_name}"
            );
        }
    }

    #[test]
    fn accepts_only_hh_mm_clock_times() {
        for accepted_time in ["00:00", "02:00", "16:00", "23:59"] {
            assert_eq!(
                validate_clock_time(accepted_time).ok().as_deref(),
                Some(accepted_time)
            );
        }
        for refused_time in [
            "", "9:30", "24:00", "23:60", "02:00:00", "0200", "aa:bb", "１2:00", "-1:00",
        ] {
            assert!(validate_clock_time(refused_time).is_err(), "{refused_time}");
        }
    }
}
