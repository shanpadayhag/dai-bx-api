use axum::Router;
use axum::routing::{get, patch};

use crate::core::state::AppState;
use crate::features::categories::handlers::{
    delete_category, get_categories, patch_category, post_categories,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_categories::get_categories).post(post_categories::post_categories),
        )
        .route(
            "/{category_id}",
            patch(patch_category::patch_category).delete(delete_category::delete_category),
        )
}
