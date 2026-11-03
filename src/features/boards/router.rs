use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;
use crate::features::boards::handlers::{
    delete_board, get_board, get_boards, patch_board, post_boards,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_boards::get_boards).post(post_boards::post_boards),
        )
        .route(
            "/{board_id}",
            get(get_board::get_board)
                .patch(patch_board::patch_board)
                .delete(delete_board::delete_board),
        )
}
