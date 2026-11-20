use axum::Router;
use axum::routing::post;

use crate::core::state::AppState;
use crate::features::planning::handlers::{post_plan_accept, post_plan_proposal};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proposal", post(post_plan_proposal::post_plan_proposal))
        .route("/accept", post(post_plan_accept::post_plan_accept))
}
