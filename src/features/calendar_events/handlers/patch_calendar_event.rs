use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::update_calendar_event::UpdateCalendarEvent;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEventChanges, parse_event_instant, validate_planned_title,
};
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCalendarEventRequest {
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub title: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub category_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub starts_at: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub ends_at: Option<Option<String>>,
}

fn parse_present_instant(
    present_text: Option<Option<String>>,
    field_name: &str,
) -> Result<Option<UtcInstant>, CalendarEventError> {
    present_text
        .map(|instant_text| {
            parse_event_instant(instant_text.as_deref().unwrap_or_default(), field_name)
        })
        .transpose()
}

pub async fn patch_calendar_event(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(event_id_text): Path<String>,
    Json(patch_request): Json<PatchCalendarEventRequest>,
) -> Result<Json<CalendarEventResponse>, AppError> {
    let event_id = Uuid::parse_str(&event_id_text).map_err(|_| AppError::NotFound)?;
    let event_changes = CalendarEventChanges {
        title: patch_request
            .title
            .map(|title| validate_planned_title(title.as_deref().unwrap_or_default()))
            .transpose()?,
        category_change: patch_request
            .category_id
            .map(|category_id| {
                category_id
                    .as_deref()
                    .map(|category_id_text| {
                        Uuid::parse_str(category_id_text).map_err(|_| AppError::NotFound)
                    })
                    .transpose()
            })
            .transpose()?,
        starts_at: parse_present_instant(patch_request.starts_at, "startsAt")?,
        ends_at: parse_present_instant(patch_request.ends_at, "endsAt")?,
    };
    let updated_event = UpdateCalendarEvent::new(&app_state.db_pool)
        .execute(write_session.user_id, event_id, &event_changes)
        .await?;
    Ok(Json(updated_event.into()))
}
