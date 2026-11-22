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
use crate::core::middlewares::origin_secret::require_origin_secret;
use crate::core::middlewares::refusal_log::log_refused_request;
use crate::core::middlewares::response_headers::add_response_headers;
use crate::core::state::AppState;
use crate::features::{
    account, activity, auth, board_lists, board_properties, boards, calendar_events,
    card_schedules, cards, categories, health, notifications, planning, search, task_groups, tasks,
    today, tracking,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

pub fn app(app_state: AppState) -> Router {
    let middleware_stack = ServiceBuilder::new()
        .layer(middleware::from_fn(add_response_headers))
        .layer(middleware::from_fn(log_refused_request))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            require_origin_secret,
        ))
        .layer(middleware::from_fn(attach_client_ip))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            require_same_origin_json,
        ));

    Router::new()
        .nest("/account", account::router::router())
        .nest("/activity", activity::router::router())
        .nest("/auth", auth::router::router())
        .nest("/boards", boards::router::router())
        .nest("/calendar", calendar_events::router::router())
        .nest("/categories", categories::router::router())
        .nest("/health", health::router::router())
        .nest("/plans", planning::router::router())
        .nest("/search", search::router::router())
        .nest("/task-groups", task_groups::router::router())
        .nest("/today", today::router::router())
        .merge(board_lists::router::router())
        .merge(board_properties::router::router())
        .merge(card_schedules::router::router())
        .merge(cards::router::router())
        .merge(tasks::router::router())
        .merge(tracking::router::router())
        .merge(notifications::router::router())
        .layer(middleware_stack)
        .with_state(app_state)
}
