use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::recurrence::RepeatRule;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::create_planned_event::CreatePlannedEvent;
use crate::features::calendar_events::entities::calendar_event::{
    EventRepeat, PlannedEventInput, parse_event_moment, parse_event_zone, validate_planned_times,
    validate_planned_title,
};
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostCalendarEventRequest {
    pub title: String,
    pub category_id: Option<String>,
    pub start: String,
    pub end: String,
    pub input_zone: String,
    pub time_zone: Option<String>,
    pub repeat: Option<RepeatRule>,
}

pub async fn post_calendar_events(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Json(post_request): Json<PostCalendarEventRequest>,
) -> Result<(StatusCode, Json<CalendarEventResponse>), AppError> {
    let input_zone = parse_event_zone(&post_request.input_zone, "inputZone")?;
    let starts_at = parse_event_moment(&post_request.start, "start", &input_zone)?;
    let ends_at = parse_event_moment(&post_request.end, "end", &input_zone)?;
    validate_planned_times(starts_at, ends_at)?;
    let planned_event_input = PlannedEventInput {
        title: validate_planned_title(&post_request.title)?,
        category_id: post_request
            .category_id
            .as_deref()
            .map(|category_id_text| {
                Uuid::parse_str(category_id_text).map_err(|_| AppError::NotFound)
            })
            .transpose()?,
        starts_at,
        ends_at,
        time_zone: post_request
            .time_zone
            .as_deref()
            .map(|event_zone_name| {
                parse_event_zone(event_zone_name, "timeZone").map(|event_zone| event_zone.name)
            })
            .transpose()?,
        repeat: post_request.repeat.map(|repeat_rule| EventRepeat {
            repeat_rule,
            input_zone,
        }),
    };
    let created_event = CreatePlannedEvent::new(&app_state.db_pool)
        .execute(write_session.user_id, &planned_event_input)
        .await?;
    Ok((StatusCode::CREATED, Json(created_event.into())))
}
