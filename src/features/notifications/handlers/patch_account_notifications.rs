use axum::Json;
use axum::extract::State;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::notifications::actions::list_push_devices::ListPushDevices;
use crate::features::notifications::actions::update_notification_settings::UpdateNotificationSettings;
use crate::features::notifications::entities::notification_settings::NotificationChanges;
use crate::features::notifications::responses::notification_settings_response::NotificationSettingsResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatchNotificationsRequest {
    pub check_ins: Option<bool>,
    pub schedule_runs: Option<bool>,
    pub blocks_starting: Option<bool>,
    pub quiet_outside_working_hours: Option<bool>,
}

pub async fn patch_account_notifications(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Json(patch_request): Json<PatchNotificationsRequest>,
) -> Result<Json<NotificationSettingsResponse>, AppError> {
    let notification_settings = UpdateNotificationSettings::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            &NotificationChanges {
                check_ins: patch_request.check_ins,
                schedule_runs: patch_request.schedule_runs,
                blocks_starting: patch_request.blocks_starting,
                quiet_outside_working_hours: patch_request.quiet_outside_working_hours,
            },
        )
        .await?;
    let push_devices = ListPushDevices::new(&app_state.db_pool)
        .execute(write_session.user_id)
        .await?;
    Ok(Json(NotificationSettingsResponse::new(
        &app_state.app_config.vapid_settings.application_server_key,
        notification_settings,
        push_devices,
    )))
}
