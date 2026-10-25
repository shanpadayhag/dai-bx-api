use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::sign_up::{SignUp, SignUpDetails};
use crate::features::auth::responses::status_response::StatusResponse;

const MAX_NAME_CHARACTERS: usize = 100;
const MAX_EMAIL_CHARACTERS: usize = 254;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignUpRequest {
    pub invite_code: Option<String>,
    pub name: String,
    pub email: String,
    pub password: String,
}

pub async fn post_sign_up(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    Json(sign_up_request): Json<SignUpRequest>,
) -> Result<(StatusCode, Json<StatusResponse>), AppError> {
    let name = validate_name(&sign_up_request.name)?;
    let email = validate_email(&sign_up_request.email)?;
    SignUp::new(&app_state)
        .execute(
            SignUpDetails {
                invite_code: sign_up_request.invite_code.as_deref(),
                name,
                email,
                password: Zeroizing::new(sign_up_request.password),
            },
            &request_context,
        )
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StatusResponse::verification_email_sent()),
    ))
}

pub(crate) fn validate_name(entered_name: &str) -> Result<&str, AppError> {
    let trimmed_name = entered_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > MAX_NAME_CHARACTERS {
        return Err(AppError::Validation(format!(
            "name must be 1 to {MAX_NAME_CHARACTERS} characters"
        )));
    }
    Ok(trimmed_name)
}

pub(crate) fn validate_email(entered_email: &str) -> Result<&str, AppError> {
    let trimmed_email = entered_email.trim();
    let is_valid_address = trimmed_email.len() <= MAX_EMAIL_CHARACTERS
        && trimmed_email.parse::<lettre::Address>().is_ok();
    if !is_valid_address {
        return Err(AppError::Validation(String::from(
            "enter a valid email address",
        )));
    }
    Ok(trimmed_email)
}
