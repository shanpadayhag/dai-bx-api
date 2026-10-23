use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;
use crate::features::health::handlers::get_health;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_health::get_health))
}
