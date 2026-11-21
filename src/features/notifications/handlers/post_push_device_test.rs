use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::notifications::actions::send_test_notification::SendTestNotification;
use crate::features::notifications::responses::sent_response::SentResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn post_push_device_test(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(device_id_text): Path<String>,
) -> Result<Json<SentResponse>, AppError> {
    let device_id = Uuid::parse_str(&device_id_text).map_err(|_| AppError::NotFound)?;
    SendTestNotification::new(
        &app_state.db_pool,
        &app_state.http_client,
        &app_state.app_config.vapid_settings,
    )
    .execute(write_session.user_id, device_id)
    .await?;
    Ok(Json(SentResponse::sent()))
}
