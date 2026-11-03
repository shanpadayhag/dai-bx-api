use axum::Router;
use axum::routing::{get, patch, post};

use crate::core::state::AppState;
use crate::features::board_lists::handlers::{
    delete_list, get_list_export, patch_list, post_board_lists, post_list_move,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/boards/{board_id}/lists",
            post(post_board_lists::post_board_lists),
        )
        .route(
            "/lists/{list_id}",
            patch(patch_list::patch_list).delete(delete_list::delete_list),
        )
        .route(
            "/lists/{list_id}/move",
            post(post_list_move::post_list_move),
        )
        .route(
            "/lists/{list_id}/export",
            get(get_list_export::get_list_export),
        )
}
