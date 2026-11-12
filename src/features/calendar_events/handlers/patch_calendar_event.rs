use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::recurrence::RepeatRule;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::update_calendar_event::UpdateCalendarEvent;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEventChanges, EventRepeat, NamedTimeZone, parse_event_moment, parse_event_zone,
    validate_planned_title,
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
    pub start: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub end: Option<Option<String>>,
    pub input_zone: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub time_zone: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub repeat: Option<Option<RepeatRule>>,
}

fn require_input_zone(
    input_zone: Option<&NamedTimeZone>,
) -> Result<&NamedTimeZone, CalendarEventError> {
    input_zone.ok_or_else(|| {
        CalendarEventError::Invalid(String::from(
            "inputZone is needed with a new start, end or repeat",
        ))
    })
}

fn parse_present_moment(
    present_text: Option<Option<String>>,
    field_name: &str,
    input_zone: Option<&NamedTimeZone>,
) -> Result<Option<UtcInstant>, CalendarEventError> {
    present_text
        .map(|moment_text| {
            parse_event_moment(
                moment_text.as_deref().unwrap_or_default(),
                field_name,
                require_input_zone(input_zone)?,
            )
        })
        .transpose()
}

impl PatchCalendarEventRequest {
    pub fn into_event_changes(
        self,
    ) -> Result<(CalendarEventChanges, Option<NamedTimeZone>), AppError> {
        let input_zone = self
            .input_zone
            .as_deref()
            .map(|input_zone_name| parse_event_zone(input_zone_name, "inputZone"))
            .transpose()?;
        let event_changes = CalendarEventChanges {
            title: self
                .title
                .map(|title| validate_planned_title(title.as_deref().unwrap_or_default()))
                .transpose()?,
            category_change: self
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
            starts_at: parse_present_moment(self.start, "start", input_zone.as_ref())?,
            ends_at: parse_present_moment(self.end, "end", input_zone.as_ref())?,
            time_zone_change: self
                .time_zone
                .map(|event_zone_name| {
                    event_zone_name
                        .as_deref()
                        .map(|event_zone_name| {
                            parse_event_zone(event_zone_name, "timeZone")
                                .map(|event_zone| event_zone.name)
                        })
                        .transpose()
                })
                .transpose()?,
            repeat_change: self
                .repeat
                .map(|repeat_rule| {
                    repeat_rule
                        .map(|repeat_rule| {
                            require_input_zone(input_zone.as_ref()).map(|input_zone| EventRepeat {
                                repeat_rule,
                                input_zone: input_zone.clone(),
                            })
                        })
                        .transpose()
                })
                .transpose()?,
        };
        Ok((event_changes, input_zone))
    }
}

pub async fn patch_calendar_event(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(event_id_text): Path<String>,
    Json(patch_request): Json<PatchCalendarEventRequest>,
) -> Result<Json<CalendarEventResponse>, AppError> {
    let event_id = Uuid::parse_str(&event_id_text).map_err(|_| AppError::NotFound)?;
    let (event_changes, _) = patch_request.into_event_changes()?;
    let updated_event = UpdateCalendarEvent::new(&app_state.db_pool)
        .execute(write_session.user_id, event_id, &event_changes)
        .await?;
    Ok(Json(updated_event.into()))
}
