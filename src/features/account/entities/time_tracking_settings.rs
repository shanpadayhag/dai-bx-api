use uuid::Uuid;

use crate::features::account::error::AccountError;

pub const CHECK_IN_MINUTES_RANGE: std::ops::RangeInclusive<i32> = 5..=240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeTrackingSettings {
    pub check_ins_enabled: bool,
    pub check_in_interval_minutes: i32,
    pub first_check_in_minutes: i32,
    pub working_hours_start: String,
    pub working_hours_end: String,
    pub tracked_category_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeTrackingChanges {
    pub check_ins_enabled: Option<bool>,
    pub check_in_interval_minutes: Option<i32>,
    pub first_check_in_minutes: Option<i32>,
    pub working_hours_start: Option<String>,
    pub working_hours_end: Option<String>,
    pub tracked_category_change: Option<Option<Uuid>>,
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
