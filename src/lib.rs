pub mod core;
pub mod features;

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::middleware;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

use crate::core::middlewares::client_ip::attach_client_ip;
use crate::core::middlewares::csrf_protection::require_same_origin_json;
use crate::core::state::AppState;
use crate::features::{account, auth, health};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

pub fn app(app_state: AppState) -> Router {
    let middleware_stack = ServiceBuilder::new()
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn(attach_client_ip))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            require_same_origin_json,
        ));

    Router::new()
        .nest("/account", account::router::router())
        .nest("/auth", auth::router::router())
        .nest("/health", health::router::router())
        .layer(middleware_stack)
        .with_state(app_state)
}
