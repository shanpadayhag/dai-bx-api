use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::core::cookie::build_host_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::verify_email::VerifyEmail;
use crate::features::auth::responses::next_step_response::NextStepResponse;
use crate::features::auth_challenges::entities::auth_challenge::{
    AUTH_CHALLENGE_COOKIE_NAME, AUTH_CHALLENGE_VALID_FOR_SECONDS,
};

#[derive(Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

pub async fn post_verify_email(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    Json(verify_email_request): Json<VerifyEmailRequest>,
) -> Result<Response, AppError> {
    let challenge_token = VerifyEmail::new(&app_state)
        .execute(verify_email_request.token.trim(), &request_context)
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
