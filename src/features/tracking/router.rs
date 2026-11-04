use axum::Router;
use axum::routing::{get, post};

use crate::core::state::AppState;
use crate::features::tracking::handlers::{
    get_running_items, post_card_start, post_check_in, post_finish, post_task_start,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/tasks/{task_id}/start",
            post(post_task_start::post_task_start),
        )
        .route(
            "/cards/{card_id}/start",
            post(post_card_start::post_card_start),
        )
        .route(
            "/tracking/running",
            get(get_running_items::get_running_items),
        )
        .route(
            "/tracking/{event_id}/check-in",
            post(post_check_in::post_check_in),
        )
        .route(
            "/tracking/{event_id}/finish",
            post(post_finish::post_finish),
        )
}
