use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{AppendHeaders, IntoResponse, Response};

use crate::core::cookie::build_expired_host_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::sign_out::SignOut;
use crate::features::auth::responses::status_response::StatusResponse;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::{ACCESS_COOKIE_NAME, REFRESH_COOKIE_NAME};

pub async fn post_sign_out(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
) -> Result<Response, AppError> {
    SignOut::new(&app_state)
        .execute(current_session, &request_context)
        .await?;
    Ok((
        AppendHeaders([
            (
                header::SET_COOKIE,
                build_expired_host_cookie(ACCESS_COOKIE_NAME),
            ),
            (
                header::SET_COOKIE,
                build_expired_host_cookie(REFRESH_COOKIE_NAME),
            ),
        ]),
        Json(StatusResponse::signed_out()),
    )
        .into_response())
}
