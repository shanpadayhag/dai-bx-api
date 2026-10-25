use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Response};

use crate::core::cookie::read_cookie;
use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::refresh_tokens::RefreshTokens;
use crate::features::auth::responses::status_response::StatusResponse;
use crate::features::sessions::entities::session::REFRESH_COOKIE_NAME;

pub async fn post_refresh(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    request_headers: HeaderMap,
) -> Result<Response, AppError> {
    let refresh_token =
        read_cookie(&request_headers, REFRESH_COOKIE_NAME).ok_or(AppError::Unauthorized)?;
    let session_tokens = RefreshTokens::new(&app_state)
        .execute(refresh_token, &request_context)
        .await?;
    let [access_cookie, refresh_cookie] = session_tokens.to_set_cookie_values();
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, access_cookie),
            (header::SET_COOKIE, refresh_cookie),
        ]),
        Json(StatusResponse::refreshed()),
    )
        .into_response())
}
