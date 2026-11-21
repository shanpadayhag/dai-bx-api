use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;
use crate::features::today::handlers::get_up_next;

pub fn router() -> Router<AppState> {
    Router::new().route("/up-next", get(get_up_next::get_up_next))
}
