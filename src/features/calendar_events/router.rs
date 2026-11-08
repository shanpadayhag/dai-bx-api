use axum::Router;
use axum::routing::{get, patch};

use crate::core::state::AppState;
use crate::features::calendar_events::handlers::{
    delete_calendar_event, delete_series_occurrence, get_calendar_events, patch_calendar_event,
    patch_series_occurrence, post_calendar_events,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/events",
            get(get_calendar_events::get_calendar_events)
                .post(post_calendar_events::post_calendar_events),
        )
        .route(
            "/events/{event_id}",
            patch(patch_calendar_event::patch_calendar_event)
                .delete(delete_calendar_event::delete_calendar_event),
        )
        .route(
            "/series/{series_id}/occurrences/{original_start}",
            patch(patch_series_occurrence::patch_series_occurrence)
                .delete(delete_series_occurrence::delete_series_occurrence),
        )
}
