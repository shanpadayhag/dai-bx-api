use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::notifications::actions::find_notification_settings::FindNotificationSettings;
use crate::features::notifications::actions::list_push_devices::ListPushDevices;
use crate::features::notifications::responses::notification_settings_response::NotificationSettingsResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_account_notifications(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<NotificationSettingsResponse>, AppError> {
    let notification_settings = FindNotificationSettings::new(&app_state.db_pool)
        .execute(current_session.user_id)
        .await?;
    let push_devices = ListPushDevices::new(&app_state.db_pool)
        .execute(current_session.user_id)
        .await?;
    Ok(Json(NotificationSettingsResponse::new(
        &app_state.app_config.vapid_settings.application_server_key,
        notification_settings,
        push_devices,
    )))
}
