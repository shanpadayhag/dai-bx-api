use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;
use crate::features::activity::handlers::get_activity;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_activity::get_activity))
}
