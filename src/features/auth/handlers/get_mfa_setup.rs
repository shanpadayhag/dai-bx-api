use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

use crate::core::cookie::read_cookie;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::auth::actions::show_authenticator_setup::ShowAuthenticatorSetup;
use crate::features::auth::responses::authenticator_setup_response::AuthenticatorSetupResponse;
use crate::features::auth_challenges::entities::auth_challenge::AUTH_CHALLENGE_COOKIE_NAME;

pub async fn get_mfa_setup(
    State(app_state): State<AppState>,
    request_headers: HeaderMap,
) -> Result<Json<AuthenticatorSetupResponse>, AppError> {
    let challenge_token =
        read_cookie(&request_headers, AUTH_CHALLENGE_COOKIE_NAME).ok_or(AppError::Unauthorized)?;
    let authenticator_setup = ShowAuthenticatorSetup::new(&app_state)
        .execute(challenge_token)
        .await?;
    Ok(Json(authenticator_setup.into()))
}
