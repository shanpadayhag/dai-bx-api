use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::cookie::read_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::sign_in::{SignIn, SignInDetails};
use crate::features::auth::handlers::post_sign_up::validate_email;
use crate::features::auth::responses::sign_in_outcome_response::sign_in_outcome_response;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_COOKIE_NAME;
use crate::features::sessions::entities::presented_session::PresentedSession;

#[derive(Deserialize)]
pub struct SignInRequest {
    pub email: String,
    pub password: String,
}

pub async fn post_sign_in(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    presented_session: PresentedSession,
    request_headers: HeaderMap,
    Json(sign_in_request): Json<SignInRequest>,
) -> Result<Response, AppError> {
    let email = validate_email(&sign_in_request.email)?;
    let sign_in_outcome = SignIn::new(&app_state)
        .execute(
            SignInDetails {
                email,
                password: Zeroizing::new(sign_in_request.password),
                device_token: read_cookie(&request_headers, REMEMBERED_DEVICE_COOKIE_NAME),
            },
            &request_context,
            &presented_session,
        )
        .await?;
    Ok(sign_in_outcome_response(sign_in_outcome, Vec::new()))
}
