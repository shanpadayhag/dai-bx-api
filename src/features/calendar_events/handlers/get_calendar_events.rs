use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::list_calendar_events::ListCalendarEvents;
use crate::features::calendar_events::entities::calendar_event::{
    parse_event_zone, validate_event_range,
};
use crate::features::calendar_events::responses::calendar_range_response::CalendarRangeResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCalendarEventsQuery {
    pub start: Option<String>,
    pub days: Option<String>,
    pub time_zone: Option<String>,
}

pub async fn get_calendar_events(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Query(events_query): Query<GetCalendarEventsQuery>,
) -> Result<Json<CalendarRangeResponse>, AppError> {
    let range_zone = parse_event_zone(
        events_query.time_zone.as_deref().unwrap_or_default(),
        "timeZone",
    )?;
    let event_range = validate_event_range(
        events_query.start.as_deref().unwrap_or_default(),
        events_query.days.as_deref().unwrap_or_default(),
        &range_zone,
    )?;
    let calendar_range = ListCalendarEvents::new(&app_state.db_pool)
        .execute(current_session.user_id, event_range)
        .await?;
    Ok(Json(calendar_range.into()))
}
