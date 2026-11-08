use axum::Router;
use axum::routing::{get, patch, post};

use crate::core::state::AppState;
use crate::features::card_schedules::handlers::{
    delete_schedule, get_schedule_runs, patch_schedule, post_card_schedules, post_schedule_pause,
    post_schedule_run_now,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/cards/{card_id}/schedules",
            post(post_card_schedules::post_card_schedules),
        )
        .route(
            "/schedules/{schedule_id}",
            patch(patch_schedule::patch_schedule).delete(delete_schedule::delete_schedule),
        )
        .route(
            "/schedules/{schedule_id}/pause",
            post(post_schedule_pause::post_schedule_pause),
        )
        .route(
            "/schedules/{schedule_id}/resume",
            post(post_schedule_pause::post_schedule_resume),
        )
        .route(
            "/schedules/{schedule_id}/run-now",
            post(post_schedule_run_now::post_schedule_run_now),
        )
        .route(
            "/schedules/{schedule_id}/runs",
            get(get_schedule_runs::get_schedule_runs),
        )
}
