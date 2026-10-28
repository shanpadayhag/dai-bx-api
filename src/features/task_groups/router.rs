use axum::Router;
use axum::routing::{get, patch};

use crate::core::state::AppState;
use crate::features::task_groups::handlers::{
    delete_task_group, get_task_groups, patch_task_group, post_task_groups,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_task_groups::get_task_groups).post(post_task_groups::post_task_groups),
        )
        .route(
            "/{task_group_id}",
            patch(patch_task_group::patch_task_group).delete(delete_task_group::delete_task_group),
        )
}
