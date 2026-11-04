use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::list_calendar_events::ListCalendarEvents;
use crate::features::calendar_events::entities::calendar_event::{
    parse_event_instant, validate_event_range,
};
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetCalendarEventsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn get_calendar_events(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Query(events_query): Query<GetCalendarEventsQuery>,
) -> Result<Json<Vec<CalendarEventResponse>>, AppError> {
    let from = parse_event_instant(events_query.from.as_deref().unwrap_or_default(), "from")?;
    let to = parse_event_instant(events_query.to.as_deref().unwrap_or_default(), "to")?;
    let event_range = validate_event_range(from, to)?;
    let listed_events = ListCalendarEvents::new(&app_state.db_pool)
        .execute(current_session.user_id, event_range)
        .await?;
    Ok(Json(
        listed_events
            .into_iter()
            .map(CalendarEventResponse::from)
            .collect(),
    ))
}
