use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::change_password::{ChangePassword, PasswordChange};
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn post_account_password(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(change_request): Json<ChangePasswordRequest>,
) -> Result<Json<StatusResponse>, AppError> {
    ChangePassword::new(&app_state)
        .execute(
            current_session,
            PasswordChange {
                current_password: Zeroizing::new(change_request.current_password),
                new_password: Zeroizing::new(change_request.new_password),
            },
            &request_context,
        )
        .await?;
    Ok(Json(StatusResponse::password_changed()))
}
