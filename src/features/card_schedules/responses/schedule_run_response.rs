use serde::Serialize;

use crate::features::card_schedules::entities::schedule_run::{RunTiming, ScheduleRun};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRunResponse {
    pub id: String,
    pub ran_at: String,
    pub how: &'static str,
    pub moved: bool,
    pub tracked_minutes: i64,
}

impl From<ScheduleRun> for ScheduleRunResponse {
    fn from(schedule_run: ScheduleRun) -> Self {
        Self {
            id: schedule_run.id.to_string(),
            ran_at: schedule_run.ran_at.to_iso_text(),
            how: match schedule_run.timing {
                RunTiming::OnTime => "onTime",
                RunTiming::Late => "late",
                RunTiming::ByHand => "byHand",
            },
            moved: schedule_run.is_moved,
            tracked_minutes: schedule_run.tracked_minutes,
        }
    }
}
