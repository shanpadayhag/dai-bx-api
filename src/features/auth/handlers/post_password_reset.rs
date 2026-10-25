use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::request_password_reset::RequestPasswordReset;
use crate::features::auth::handlers::post_sign_up::validate_email;
use crate::features::auth::responses::status_response::StatusResponse;

#[derive(Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

pub async fn post_password_reset(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    Json(reset_request): Json<PasswordResetRequest>,
) -> Result<(StatusCode, Json<StatusResponse>), AppError> {
    let email = validate_email(&reset_request.email)?;
    RequestPasswordReset::new(&app_state)
        .execute(email, &request_context)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StatusResponse::reset_email_sent()),
    ))
}
