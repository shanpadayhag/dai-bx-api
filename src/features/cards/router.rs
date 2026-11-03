use axum::Router;
use axum::routing::{get, post, put};

use crate::core::state::AppState;
use crate::features::cards::handlers::{
    delete_card, delete_card_value, get_card, patch_card, post_card_move, post_list_cards,
    put_card_value,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/lists/{list_id}/cards",
            post(post_list_cards::post_list_cards),
        )
        .route(
            "/cards/{card_id}",
            get(get_card::get_card)
                .patch(patch_card::patch_card)
                .delete(delete_card::delete_card),
        )
        .route(
            "/cards/{card_id}/move",
            post(post_card_move::post_card_move),
        )
        .route(
            "/cards/{card_id}/values/{property_id}",
            put(put_card_value::put_card_value).delete(delete_card_value::delete_card_value),
        )
}
