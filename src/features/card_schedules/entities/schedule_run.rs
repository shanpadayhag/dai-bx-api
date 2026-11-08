use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::error::ScheduleError;

pub const RUN_PAGE_LIMIT_DEFAULT: i64 = 20;
pub const RUN_PAGE_LIMIT_MAX: i64 = 100;
pub const DUE_SCHEDULES_PER_CALL: i64 = 100;
pub const LATE_AFTER_MILLIS: i64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTiming {
    OnTime,
    Late,
    ByHand,
}

impl RunTiming {
    pub fn for_occurrence(occurrence_at: UtcInstant, ran_at: UtcInstant) -> Self {
        if ran_at.unix_millis() - occurrence_at.unix_millis() > LATE_AFTER_MILLIS {
            Self::Late
        } else {
            Self::OnTime
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnTime => "on_time",
            Self::Late => "late",
            Self::ByHand => "by_hand",
        }
    }

    pub fn from_stored(stored_text: &str) -> Self {
        match stored_text {
            "late" => Self::Late,
            "by_hand" => Self::ByHand,
            _ => Self::OnTime,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunRequest {
    pub occurrence_at: UtcInstant,
    pub timing: RunTiming,
    pub now: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScheduleRun {
    pub id: Uuid,
    pub user_id: Uuid,
    pub schedule_id: Uuid,
    pub occurrence_at: UtcInstant,
    pub ran_at: UtcInstant,
    pub timing: RunTiming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleRun {
    pub id: Uuid,
    pub ran_at: UtcInstant,
    pub timing: RunTiming,
    pub is_moved: bool,
    pub tracked_minutes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunPage {
    pub before: Option<UtcInstant>,
    pub limit: i64,
}

impl RunPage {
    pub fn parse(
        before_text: Option<&str>,
        limit_text: Option<&str>,
    ) -> Result<Self, ScheduleError> {
        let before = before_text
            .map(|before_text| {
                UtcInstant::parse(before_text).ok_or_else(|| {
                    ScheduleError::Invalid(String::from(
                        "before is an instant like 2026-11-09T10:00:00.000Z",
                    ))
                })
            })
            .transpose()?;
        let limit = limit_text
            .map(|limit_text| {
                limit_text
                    .parse::<i64>()
                    .ok()
                    .filter(|limit| (1..=RUN_PAGE_LIMIT_MAX).contains(limit))
                    .ok_or_else(|| ScheduleError::Invalid(String::from("limit is 1 to 100")))
            })
            .transpose()?
            .unwrap_or(RUN_PAGE_LIMIT_DEFAULT);
        Ok(Self { before, limit })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DueRunTally {
    pub ran_count: usize,
    pub failed_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

    #[test]
    fn marks_a_run_late_only_past_two_minutes() {
        let occurrence_at = instant("2030-01-07T10:00:00Z");

        assert_eq!(
            RunTiming::for_occurrence(occurrence_at, instant("2030-01-07T10:02:00Z")),
            RunTiming::OnTime
        );
        assert_eq!(
            RunTiming::for_occurrence(occurrence_at, instant("2030-01-07T10:02:01Z")),
            RunTiming::Late
        );
        for run_timing in [RunTiming::OnTime, RunTiming::Late, RunTiming::ByHand] {
            assert_eq!(RunTiming::from_stored(run_timing.as_str()), run_timing);
        }
    }

    #[test]
    fn reads_a_page_of_1_to_100_runs_before_an_instant() {
        assert_eq!(
            RunPage::parse(None, None).ok(),
            Some(RunPage {
                before: None,
                limit: RUN_PAGE_LIMIT_DEFAULT,
            })
        );
        assert_eq!(
            RunPage::parse(Some("2030-01-07T10:00:00.000Z"), Some("100")).ok(),
            Some(RunPage {
                before: Some(instant("2030-01-07T10:00:00Z")),
                limit: 100,
            })
        );
        for (before_text, limit_text) in [
            (None, Some("0")),
            (None, Some("101")),
            (None, Some("many")),
            (Some("yesterday"), None),
        ] {
            assert!(RunPage::parse(before_text, limit_text).is_err());
        }
    }
}
