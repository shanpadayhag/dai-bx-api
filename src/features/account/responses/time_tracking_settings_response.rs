use serde::Serialize;

use crate::features::account::entities::time_tracking_settings::{
    PlanningDay, TimeTrackingSettings,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeTrackingSettingsResponse {
    pub check_ins_enabled: bool,
    pub check_in_interval_minutes: i32,
    pub first_check_in_minutes: i32,
    pub working_hours_start: String,
    pub working_hours_end: String,
    pub tracked_category_id: Option<String>,
    pub calendar_time_zone: Option<String>,
    pub planning_hours: Option<Vec<Option<PlanningDayResponse>>>,
}

#[derive(Debug, Serialize)]
pub struct PlanningDayResponse {
    pub start: String,
    pub end: String,
}

impl From<PlanningDay> for PlanningDayResponse {
    fn from(planning_day: PlanningDay) -> Self {
        Self {
            start: planning_day.start,
            end: planning_day.end,
        }
    }
}

impl From<TimeTrackingSettings> for TimeTrackingSettingsResponse {
    fn from(time_tracking_settings: TimeTrackingSettings) -> Self {
        Self {
            check_ins_enabled: time_tracking_settings.check_ins_enabled,
            check_in_interval_minutes: time_tracking_settings.check_in_interval_minutes,
            first_check_in_minutes: time_tracking_settings.first_check_in_minutes,
            working_hours_start: time_tracking_settings.working_hours_start,
            working_hours_end: time_tracking_settings.working_hours_end,
            tracked_category_id: time_tracking_settings
                .tracked_category_id
                .map(|tracked_category_id| tracked_category_id.to_string()),
            calendar_time_zone: time_tracking_settings.calendar_time_zone,
            planning_hours: time_tracking_settings.planning_hours.map(|planning_week| {
                planning_week
                    .0
                    .into_iter()
                    .map(|planning_day| planning_day.map(PlanningDayResponse::from))
                    .collect()
            }),
        }
    }
}
