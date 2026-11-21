use crate::core::utc_instant::UtcInstant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckInSettings {
    pub enabled: bool,
    pub interval_minutes: i64,
    pub first_check_in_minutes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunningCheckIn {
    pub started_at: UtcInstant,
    pub last_answered_at: Option<UtcInstant>,
    pub estimate_minutes: Option<i64>,
}

pub fn check_in_due_at(
    running_check_in: &RunningCheckIn,
    check_in_settings: &CheckInSettings,
) -> Option<UtcInstant> {
    if !check_in_settings.enabled {
        return None;
    }
    Some(match running_check_in.last_answered_at {
        Some(last_answered_at) => last_answered_at.plus_minutes(check_in_settings.interval_minutes),
        None => running_check_in.started_at.plus_minutes(
            running_check_in
                .estimate_minutes
                .unwrap_or(check_in_settings.first_check_in_minutes),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("test instants are valid")
    }

    const CHECK_IN_SETTINGS: CheckInSettings = CheckInSettings {
        enabled: true,
        interval_minutes: 30,
        first_check_in_minutes: 20,
    };

    fn running_check_in() -> RunningCheckIn {
        RunningCheckIn {
            started_at: instant("2026-11-05T16:00:00Z"),
            last_answered_at: None,
            estimate_minutes: Some(45),
        }
    }

    #[test]
    fn the_first_check_in_comes_when_the_estimate_runs_out() {
        assert_eq!(
            check_in_due_at(&running_check_in(), &CHECK_IN_SETTINGS),
            Some(instant("2026-11-05T16:45:00Z"))
        );
    }

    #[test]
    fn without_an_estimate_the_first_check_in_comes_after_the_first_check_in_setting() {
        let unestimated_check_in = RunningCheckIn {
            estimate_minutes: None,
            ..running_check_in()
        };

        assert_eq!(
            check_in_due_at(&unestimated_check_in, &CHECK_IN_SETTINGS),
            Some(instant("2026-11-05T16:20:00Z"))
        );
    }

    #[test]
    fn after_a_yes_the_next_one_comes_one_interval_after_that_answer() {
        let answered_check_in = RunningCheckIn {
            last_answered_at: Some(instant("2026-11-05T16:50:00Z")),
            ..running_check_in()
        };

        assert_eq!(
            check_in_due_at(&answered_check_in, &CHECK_IN_SETTINGS),
            Some(instant("2026-11-05T17:20:00Z"))
        );
    }

    #[test]
    fn nothing_is_due_while_check_ins_are_off() {
        let switched_off_settings = CheckInSettings {
            enabled: false,
            ..CHECK_IN_SETTINGS
        };

        assert_eq!(
            check_in_due_at(&running_check_in(), &switched_off_settings),
            None
        );
    }
}
