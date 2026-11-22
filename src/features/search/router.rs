use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;
use crate::features::search::handlers::get_search;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_search::get_search))
}
