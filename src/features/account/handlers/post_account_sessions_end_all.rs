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
use crate::features::account::actions::end_all_sessions::EndAllSessions;
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::remembered_devices::entities::remembered_device::REMEMBERED_DEVICE_COOKIE_NAME;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::{ACCESS_COOKIE_NAME, REFRESH_COOKIE_NAME};

#[derive(Deserialize)]
pub struct EndAllSessionsRequest {
    pub password: String,
}

pub async fn post_account_sessions_end_all(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(end_all_request): Json<EndAllSessionsRequest>,
) -> Result<Response, AppError> {
    EndAllSessions::new(&app_state)
        .execute(
            current_session,
            Zeroizing::new(end_all_request.password),
            &request_context,
        )
        .await?;
    Ok((
        AppendHeaders(
            [
                ACCESS_COOKIE_NAME,
                REFRESH_COOKIE_NAME,
                REMEMBERED_DEVICE_COOKIE_NAME,
            ]
            .map(|cookie_name| (header::SET_COOKIE, build_expired_host_cookie(cookie_name))),
        ),
        Json(StatusResponse::signed_out()),
    )
        .into_response())
}
