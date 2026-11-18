use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use uuid::Uuid;

use crate::core::recurrence::parse_time_zone;

pub const RECOVERY_CODE_SIGN_IN_METHOD: &str = "recovery_code";
pub const SIGN_IN_ALERT_RUN_BUDGET: Duration = Duration::from_secs(20);
pub const SIGN_IN_ALERT_RETRY_AFTER_SECONDS: i64 = 60;
pub const SIGN_IN_ALERT_RETRY_WINDOW_SECONDS: i64 = 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewSignInAlert<'a> {
    pub user_id: Uuid,
    pub user_agent: Option<&'a str>,
    pub sign_in_method: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSignInAlert {
    pub id: Uuid,
    pub recipient: String,
    pub security_ref: Uuid,
    pub calendar_time_zone: Option<String>,
    pub user_agent: Option<String>,
    pub sign_in_method: String,
    pub signed_in_unix_millis: i64,
    pub is_past_retry_window: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SignInAlertTally {
    pub sent_count: u32,
    pub retry_count: u32,
    pub dropped_count: u32,
}

pub fn describe_sign_in_time(
    signed_in_unix_millis: i64,
    calendar_time_zone: Option<&str>,
) -> String {
    let time_zone = calendar_time_zone
        .and_then(parse_time_zone)
        .unwrap_or(TimeZone::UTC);
    let time_zone_label = time_zone.iana_name().unwrap_or("UTC").to_owned();
    let signed_in_moment = Timestamp::from_millisecond(signed_in_unix_millis)
        .expect("a stored sign-in time is within the supported range")
        .to_zoned(time_zone);
    format!(
        "{} ({time_zone_label})",
        signed_in_moment.strftime("%a, %-d %b %Y, %-I:%M %p")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN_UNIX_MILLIS: i64 = 1_795_009_260_000;

    #[test]
    fn describes_the_time_in_the_account_time_zone() {
        assert_eq!(
            describe_sign_in_time(SIGNED_IN_UNIX_MILLIS, Some("Asia/Manila")),
            "Wed, 18 Nov 2026, 9:41 PM (Asia/Manila)"
        );
    }

    #[test]
    fn falls_back_to_utc_without_a_usable_time_zone() {
        for calendar_time_zone in [None, Some("Not/AZone")] {
            assert_eq!(
                describe_sign_in_time(SIGNED_IN_UNIX_MILLIS, calendar_time_zone),
                "Wed, 18 Nov 2026, 1:41 PM (UTC)"
            );
        }
    }
}
