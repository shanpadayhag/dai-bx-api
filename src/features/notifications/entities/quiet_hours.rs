use jiff::tz::TimeZone;

use crate::core::recurrence::{parse_time_zone, to_local};
use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::WallClockWindow;

pub fn notification_time_zone(
    calendar_time_zone: Option<&str>,
    device_time_zone: &str,
) -> Option<TimeZone> {
    calendar_time_zone
        .and_then(parse_time_zone)
        .or_else(|| parse_time_zone(device_time_zone))
}

#[derive(Debug, Clone)]
pub struct QuietHours {
    pub working_hours: WallClockWindow,
    pub time_zone: TimeZone,
}

impl QuietHours {
    pub fn new(
        working_hours: WallClockWindow,
        calendar_time_zone: Option<&str>,
        device_time_zone: &str,
    ) -> Option<Self> {
        let time_zone = notification_time_zone(calendar_time_zone, device_time_zone)?;
        Some(Self {
            working_hours,
            time_zone,
        })
    }

    pub fn is_outside_working_hours(&self, instant: UtcInstant) -> bool {
        let Some(local_moment) = to_local(instant, &self.time_zone) else {
            return false;
        };
        let local_time = local_moment.time();
        let is_inside = if self.working_hours.crosses_midnight() {
            local_time >= self.working_hours.start || local_time < self.working_hours.end
        } else {
            self.working_hours.start <= local_time && local_time < self.working_hours.end
        };
        !is_inside
    }

    pub fn holds_back(&self, due_at: UtcInstant, check_in_started_at: Option<UtcInstant>) -> bool {
        let started_outside_working_hours =
            check_in_started_at.is_some_and(|started_at| self.is_outside_working_hours(started_at));
        self.is_outside_working_hours(due_at) && !started_outside_working_hours
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::time;

    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("test instants are valid")
    }

    fn evening_quiet_hours(calendar_time_zone: Option<&str>) -> QuietHours {
        QuietHours::new(
            WallClockWindow::new(time(16, 0, 0, 0), time(2, 0, 0, 0)).expect("hours differ"),
            calendar_time_zone,
            "Asia/Manila",
        )
        .expect("zones are valid")
    }

    #[test]
    fn working_hours_that_wrap_past_midnight_include_both_sides() {
        let quiet_hours = evening_quiet_hours(None);

        assert!(!quiet_hours.is_outside_working_hours(instant("2026-11-22T08:00:00Z")));
        assert!(!quiet_hours.is_outside_working_hours(instant("2026-11-22T17:59:00Z")));
        assert!(quiet_hours.is_outside_working_hours(instant("2026-11-22T18:00:00Z")));
        assert!(quiet_hours.is_outside_working_hours(instant("2026-11-22T07:59:00Z")));
    }

    #[test]
    fn day_working_hours_end_before_their_end_time() {
        let quiet_hours = QuietHours::new(
            WallClockWindow::new(time(9, 0, 0, 0), time(17, 0, 0, 0)).expect("hours differ"),
            None,
            "UTC",
        )
        .expect("zones are valid");

        assert!(quiet_hours.is_outside_working_hours(instant("2026-11-22T08:59:00Z")));
        assert!(!quiet_hours.is_outside_working_hours(instant("2026-11-22T09:00:00Z")));
        assert!(quiet_hours.is_outside_working_hours(instant("2026-11-22T17:00:00Z")));
    }

    #[test]
    fn the_calendar_zone_wins_over_the_device_zone() {
        let quiet_hours = evening_quiet_hours(Some("Europe/Berlin"));

        assert!(quiet_hours.is_outside_working_hours(instant("2026-11-22T08:00:00Z")));
        assert!(!quiet_hours.is_outside_working_hours(instant("2026-11-22T16:00:00Z")));
    }

    #[test]
    fn an_unknown_calendar_zone_falls_back_to_the_device_zone() {
        let quiet_hours = evening_quiet_hours(Some("Mars/Olympus"));

        assert_eq!(quiet_hours.time_zone.iana_name(), Some("Asia/Manila"));
    }

    #[test]
    fn a_check_in_started_outside_working_hours_is_not_held_back() {
        let quiet_hours = evening_quiet_hours(None);
        let night_due_at = instant("2026-11-22T19:00:00Z");

        assert!(quiet_hours.holds_back(night_due_at, None));
        assert!(quiet_hours.holds_back(night_due_at, Some(instant("2026-11-22T17:30:00Z"))));
        assert!(!quiet_hours.holds_back(night_due_at, Some(instant("2026-11-22T18:30:00Z"))));
        assert!(!quiet_hours.holds_back(instant("2026-11-22T10:00:00Z"), None));
    }
}
