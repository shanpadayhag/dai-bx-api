use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::cookie::build_host_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::IdentityProof;
use crate::features::account::actions::start_authenticator_replacement::StartAuthenticatorReplacement;
use crate::features::auth::responses::next_step_response::NextStepResponse;
use crate::features::auth_challenges::entities::auth_challenge::{
    AUTH_CHALLENGE_COOKIE_NAME, AUTH_CHALLENGE_VALID_FOR_SECONDS,
};
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct StartAuthenticatorReplacementRequest {
    pub password: String,
    pub code: String,
}

pub async fn post_account_authenticator(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(start_request): Json<StartAuthenticatorReplacementRequest>,
) -> Result<Response, AppError> {
    let challenge_token = StartAuthenticatorReplacement::new(&app_state)
        .execute(
            current_session,
            IdentityProof {
                password: Zeroizing::new(start_request.password),
                entered_code: start_request.code.trim(),
            },
            &request_context,
        )
        .await?;
    Ok((
        [(
            header::SET_COOKIE,
            build_host_cookie(
                AUTH_CHALLENGE_COOKIE_NAME,
                &challenge_token,
                AUTH_CHALLENGE_VALID_FOR_SECONDS,
            ),
        )],
        Json(NextStepResponse::set_up_authenticator()),
    )
        .into_response())
}
