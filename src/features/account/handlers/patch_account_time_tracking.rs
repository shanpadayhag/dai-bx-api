use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::account::actions::update_time_tracking_settings::UpdateTimeTrackingSettings;
use crate::features::account::entities::time_tracking_settings::{
    PlanningWeek, TimeTrackingChanges, validate_calendar_time_zone, validate_check_in_minutes,
    validate_clock_time, validate_planning_day, validate_planning_week_length,
};
use crate::features::account::error::AccountError;
use crate::features::account::responses::time_tracking_settings_response::TimeTrackingSettingsResponse;
use crate::features::throttling::entities::write_session::WriteSession;

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
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub planning_hours: Option<Option<Vec<Option<PlanningDayRequest>>>>,
}

#[derive(Deserialize)]
pub struct PlanningDayRequest {
    pub start: String,
    pub end: String,
}

fn validate_planning_week(
    planning_day_requests: &[Option<PlanningDayRequest>],
) -> Result<PlanningWeek, AccountError> {
    validate_planning_week_length(planning_day_requests.len())?;
    planning_day_requests
        .iter()
        .map(|planning_day_request| {
            planning_day_request
                .as_ref()
                .map(|planning_day_request| {
                    validate_planning_day(&planning_day_request.start, &planning_day_request.end)
                })
                .transpose()
        })
        .collect()
}

pub async fn patch_account_time_tracking(
    State(app_state): State<AppState>,
    write_session: WriteSession,
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
        planning_hours_change: patch_request
            .planning_hours
            .as_ref()
            .map(|planning_hours| {
                planning_hours
                    .as_deref()
                    .map(validate_planning_week)
                    .transpose()
            })
            .transpose()?,
    };
    let time_tracking_settings = UpdateTimeTrackingSettings::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            &time_tracking_changes,
        )
        .await?;
    Ok(Json(time_tracking_settings.into()))
}
