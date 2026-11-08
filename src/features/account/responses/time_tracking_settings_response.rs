use serde::Serialize;

use crate::features::account::entities::time_tracking_settings::TimeTrackingSettings;

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
        }
    }
}
