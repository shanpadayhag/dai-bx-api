use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::actions::resend_verification_email::ResendVerificationEmail;
use crate::features::auth::handlers::post_sign_up::validate_email;
use crate::features::auth::responses::status_response::StatusResponse;

#[derive(Deserialize)]
pub struct ResendVerificationEmailRequest {
    pub email: String,
}

pub async fn post_resend_verification_email(
    State(app_state): State<AppState>,
    Json(resend_request): Json<ResendVerificationEmailRequest>,
) -> Result<(StatusCode, Json<StatusResponse>), AppError> {
    let email = validate_email(&resend_request.email)?;
    ResendVerificationEmail::new(&app_state)
        .execute(email)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StatusResponse::verification_email_sent()),
    ))
}
