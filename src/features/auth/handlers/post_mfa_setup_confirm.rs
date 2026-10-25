use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::Deserialize;

use crate::core::cookie::{build_expired_host_cookie, read_cookie};
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::confirm_authenticator_setup::ConfirmAuthenticatorSetup;
use crate::features::auth::responses::recovery_codes_response::RecoveryCodesResponse;
use crate::features::auth_challenges::entities::auth_challenge::AUTH_CHALLENGE_COOKIE_NAME;
use crate::features::sessions::entities::presented_session::PresentedSession;

#[derive(Deserialize)]
pub struct ConfirmAuthenticatorSetupRequest {
    pub code: String,
}

pub async fn post_mfa_setup_confirm(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    presented_session: PresentedSession,
    request_headers: HeaderMap,
    Json(confirm_request): Json<ConfirmAuthenticatorSetupRequest>,
) -> Result<Response, AppError> {
    let challenge_token =
        read_cookie(&request_headers, AUTH_CHALLENGE_COOKIE_NAME).ok_or(AppError::Unauthorized)?;
    let confirmed_setup = ConfirmAuthenticatorSetup::new(&app_state)
        .execute(
            challenge_token,
            confirm_request.code.trim(),
            &request_context,
            &presented_session,
        )
        .await?;
    let [access_cookie, refresh_cookie] = confirmed_setup.session_tokens.to_set_cookie_values();
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, access_cookie),
            (header::SET_COOKIE, refresh_cookie),
            (
                header::SET_COOKIE,
                build_expired_host_cookie(AUTH_CHALLENGE_COOKIE_NAME),
            ),
        ]),
        Json(RecoveryCodesResponse::from_codes(
            &confirmed_setup.recovery_codes,
        )),
    )
        .into_response())
}
