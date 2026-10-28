use axum::Router;
use axum::routing::{get, patch, post};

use crate::core::state::AppState;
use crate::features::tasks::handlers::{
    delete_task, get_group_tasks, patch_task, post_group_tasks, post_task_complete,
    post_task_group_import, post_task_move, post_task_reopen,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/task-groups/import",
            post(post_task_group_import::post_task_group_import),
        )
        .route(
            "/task-groups/{task_group_id}/tasks",
            get(get_group_tasks::get_group_tasks).post(post_group_tasks::post_group_tasks),
        )
        .route(
            "/tasks/{task_id}",
            patch(patch_task::patch_task).delete(delete_task::delete_task),
        )
        .route(
            "/tasks/{task_id}/complete",
            post(post_task_complete::post_task_complete),
        )
        .route(
            "/tasks/{task_id}/move",
            post(post_task_move::post_task_move),
        )
        .route(
            "/tasks/{task_id}/reopen",
            post(post_task_reopen::post_task_reopen),
        )
}
