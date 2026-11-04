use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::account::actions::find_time_tracking_settings::FindTimeTrackingSettings;
use crate::features::account::responses::time_tracking_settings_response::TimeTrackingSettingsResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_account_time_tracking(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<TimeTrackingSettingsResponse>, AppError> {
    let time_tracking_settings = FindTimeTrackingSettings::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(time_tracking_settings.into()))
}
