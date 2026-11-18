use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

use crate::core::error::AppError;
use crate::core::state::AppState;

const ORIGIN_SECRET_HEADER: &str = "x-daibx-origin-secret";
const HEALTH_PATH: &str = "/health";

pub async fn require_origin_secret(
    State(app_state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !is_health_check(&request) && !carries_the_origin_secret(&request, &app_state) {
        return Err(AppError::Forbidden);
    }
    Ok(next.run(request).await)
}

fn is_health_check(request: &Request) -> bool {
    request.method() == Method::GET && request.uri().path() == HEALTH_PATH
}

fn carries_the_origin_secret(request: &Request, app_state: &AppState) -> bool {
    request
        .headers()
        .get(ORIGIN_SECRET_HEADER)
        .is_some_and(|presented_secret| {
            app_state
                .app_config
                .origin_secret
                .is_matched_by(presented_secret.as_bytes())
        })
}
