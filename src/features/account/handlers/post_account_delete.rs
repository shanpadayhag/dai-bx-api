use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{AppendHeaders, IntoResponse, Response};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::cookie::build_expired_host_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::IdentityProof;
use crate::features::account::actions::delete_account::DeleteAccount;
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::auth_challenges::entities::auth_challenge::AUTH_CHALLENGE_COOKIE_NAME;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_COOKIE_NAME;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::{ACCESS_COOKIE_NAME, REFRESH_COOKIE_NAME};

#[derive(Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
    pub code: String,
    pub confirmation: String,
}

pub async fn post_account_delete(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(delete_request): Json<DeleteAccountRequest>,
) -> Result<Response, AppError> {
    DeleteAccount::new(&app_state)
        .execute(
            current_session,
            IdentityProof {
                password: Zeroizing::new(delete_request.password),
                entered_code: delete_request.code.trim(),
            },
            &delete_request.confirmation,
            &request_context,
        )
        .await?;
    Ok((
        AppendHeaders(
            [
                ACCESS_COOKIE_NAME,
                REFRESH_COOKIE_NAME,
                REMEMBERED_DEVICE_COOKIE_NAME,
                AUTH_CHALLENGE_COOKIE_NAME,
            ]
            .map(|cookie_name| (header::SET_COOKIE, build_expired_host_cookie(cookie_name))),
        ),
        Json(StatusResponse::account_deleted()),
    )
        .into_response())
}
