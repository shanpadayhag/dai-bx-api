use axum::Router;
use axum::routing::{patch, post};

use crate::core::state::AppState;
use crate::features::board_properties::handlers::{
    delete_option, delete_property, patch_option, patch_property, post_board_properties,
    post_option_move, post_property_move, post_property_options,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/boards/{board_id}/properties",
            post(post_board_properties::post_board_properties),
        )
        .route(
            "/properties/{property_id}",
            patch(patch_property::patch_property).delete(delete_property::delete_property),
        )
        .route(
            "/properties/{property_id}/move",
            post(post_property_move::post_property_move),
        )
        .route(
            "/properties/{property_id}/options",
            post(post_property_options::post_property_options),
        )
        .route(
            "/options/{option_id}",
            patch(patch_option::patch_option).delete(delete_option::delete_option),
        )
        .route(
            "/options/{option_id}/move",
            post(post_option_move::post_option_move),
        )
}
