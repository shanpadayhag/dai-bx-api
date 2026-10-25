use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::Deserialize;

use crate::core::cookie::{build_expired_host_cookie, build_host_cookie, read_cookie};
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::verify_sign_in_code::{SignInCodeEntry, VerifySignInCode};
use crate::features::auth::responses::signed_in_response::SignedInResponse;
use crate::features::auth_challenges::entities::auth_challenge::AUTH_CHALLENGE_COOKIE_NAME;
use crate::features::remembered_devices::entities::remembered_device::{
    REMEMBERED_DEVICE_COOKIE_NAME, REMEMBERED_DEVICE_VALID_FOR_SECONDS,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySignInCodeRequest {
    pub code: String,
    #[serde(default)]
    pub remember_device: bool,
}

pub async fn post_mfa_verify(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    request_headers: HeaderMap,
    Json(verify_request): Json<VerifySignInCodeRequest>,
) -> Result<Response, AppError> {
    let challenge_token =
        read_cookie(&request_headers, AUTH_CHALLENGE_COOKIE_NAME).ok_or(AppError::Unauthorized)?;
    let verified_sign_in = VerifySignInCode::new(&app_state)
        .execute(
            SignInCodeEntry {
                challenge_token,
                entered_code: verify_request.code.trim(),
                should_remember_device: verify_request.remember_device,
            },
            &request_context,
        )
        .await?;
    let [access_cookie, refresh_cookie] = verified_sign_in.session_tokens.to_set_cookie_values();
    let mut set_cookie_values = vec![
        access_cookie,
        refresh_cookie,
        build_expired_host_cookie(AUTH_CHALLENGE_COOKIE_NAME),
    ];
    if let Some(device_token) = &verified_sign_in.device_token {
        set_cookie_values.push(build_host_cookie(
            REMEMBERED_DEVICE_COOKIE_NAME,
            device_token,
            REMEMBERED_DEVICE_VALID_FOR_SECONDS,
        ));
    }
    Ok((
        AppendHeaders(
            set_cookie_values
                .into_iter()
                .map(|set_cookie_value| (header::SET_COOKIE, set_cookie_value)),
        ),
        Json(SignedInResponse::new(
            verified_sign_in.remaining_recovery_codes,
        )),
    )
        .into_response())
}
