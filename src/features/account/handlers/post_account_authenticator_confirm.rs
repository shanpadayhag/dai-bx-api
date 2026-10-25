use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::Deserialize;

use crate::core::cookie::{build_expired_host_cookie, read_cookie};
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_authenticator_replacement::ConfirmAuthenticatorReplacement;
use crate::features::account::error::AccountError;
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::auth_challenges::entities::auth_challenge::AUTH_CHALLENGE_COOKIE_NAME;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_COOKIE_NAME;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct ConfirmAuthenticatorReplacementRequest {
    pub code: String,
}

pub async fn post_account_authenticator_confirm(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    request_headers: HeaderMap,
    Json(confirm_request): Json<ConfirmAuthenticatorReplacementRequest>,
) -> Result<Response, AppError> {
    let challenge_token = read_cookie(&request_headers, AUTH_CHALLENGE_COOKIE_NAME)
        .ok_or(AccountError::StepExpired)?;
    ConfirmAuthenticatorReplacement::new(&app_state)
        .execute(
            current_session,
            challenge_token,
            confirm_request.code.trim(),
            &request_context,
        )
        .await?;
    Ok((
        AppendHeaders(
            [AUTH_CHALLENGE_COOKIE_NAME, REMEMBERED_DEVICE_COOKIE_NAME]
                .map(|cookie_name| (header::SET_COOKIE, build_expired_host_cookie(cookie_name))),
        ),
        Json(StatusResponse::authenticator_replaced()),
    )
        .into_response())
}
