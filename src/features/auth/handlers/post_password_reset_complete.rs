use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::cookie::build_expired_host_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::complete_password_reset::CompletePasswordReset;
use crate::features::auth::responses::sign_in_outcome_response::sign_in_outcome_response;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_COOKIE_NAME;
use crate::features::sessions::entities::session::{ACCESS_COOKIE_NAME, REFRESH_COOKIE_NAME};

#[derive(Deserialize)]
pub struct CompletePasswordResetRequest {
    pub token: String,
    pub password: String,
}

pub async fn post_password_reset_complete(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    Json(complete_request): Json<CompletePasswordResetRequest>,
) -> Result<Response, AppError> {
    let sign_in_outcome = CompletePasswordReset::new(&app_state)
        .execute(
            complete_request.token.trim(),
            Zeroizing::new(complete_request.password),
            &request_context,
        )
        .await?;
    Ok(sign_in_outcome_response(
        sign_in_outcome,
        [
            ACCESS_COOKIE_NAME,
            REFRESH_COOKIE_NAME,
            REMEMBERED_DEVICE_COOKIE_NAME,
        ]
        .map(build_expired_host_cookie)
        .to_vec(),
    ))
}
