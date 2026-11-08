use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::account::actions::update_time_tracking_settings::UpdateTimeTrackingSettings;
use crate::features::account::entities::time_tracking_settings::{
    TimeTrackingChanges, validate_calendar_time_zone, validate_check_in_minutes,
    validate_clock_time,
};
use crate::features::account::responses::time_tracking_settings_response::TimeTrackingSettingsResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchTimeTrackingRequest {
    pub check_ins_enabled: Option<bool>,
    pub check_in_interval_minutes: Option<i64>,
    pub first_check_in_minutes: Option<i64>,
    pub working_hours_start: Option<String>,
    pub working_hours_end: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub tracked_category_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub calendar_time_zone: Option<Option<String>>,
}

pub async fn patch_account_time_tracking(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Json(patch_request): Json<PatchTimeTrackingRequest>,
) -> Result<Json<TimeTrackingSettingsResponse>, AppError> {
    let time_tracking_changes = TimeTrackingChanges {
        check_ins_enabled: patch_request.check_ins_enabled,
        check_in_interval_minutes: patch_request
            .check_in_interval_minutes
            .map(validate_check_in_minutes)
            .transpose()?,
        first_check_in_minutes: patch_request
            .first_check_in_minutes
            .map(validate_check_in_minutes)
            .transpose()?,
        working_hours_start: patch_request
            .working_hours_start
            .as_deref()
            .map(validate_clock_time)
            .transpose()?,
        working_hours_end: patch_request
            .working_hours_end
            .as_deref()
            .map(validate_clock_time)
            .transpose()?,
        tracked_category_change: patch_request
            .tracked_category_id
            .map(|tracked_category_id| {
                tracked_category_id
                    .as_deref()
                    .map(Uuid::parse_str)
                    .transpose()
                    .map_err(|_| AppError::NotFound)
            })
            .transpose()?,
        calendar_time_zone_change: patch_request
            .calendar_time_zone
            .map(|calendar_time_zone| {
                calendar_time_zone
                    .as_deref()
                    .map(validate_calendar_time_zone)
                    .transpose()
            })
            .transpose()?,
    };
    let time_tracking_settings = UpdateTimeTrackingSettings::new()
        .execute(
            &app_state.db_pool,
            current_session.user_id,
            &time_tracking_changes,
        )
        .await?;
    Ok(Json(time_tracking_settings.into()))
}
