use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::change_series_occurrence::ChangeSeriesOccurrence;
use crate::features::calendar_events::entities::event_series::{
    OccurrenceRequest, OccurrenceScope, parse_occurrence_path,
};
use crate::features::calendar_events::handlers::patch_calendar_event::PatchCalendarEventRequest;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchSeriesOccurrenceRequest {
    scope: Option<String>,
    #[serde(flatten)]
    event_changes: PatchCalendarEventRequest,
}

pub async fn patch_series_occurrence(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path((series_id_text, original_start_text)): Path<(String, String)>,
    Json(patch_request): Json<PatchSeriesOccurrenceRequest>,
) -> Result<Json<CalendarEventResponse>, AppError> {
    let occurrence_scope =
        OccurrenceScope::parse(patch_request.scope.as_deref().unwrap_or_default())?;
    let (series_id, original_start) = parse_occurrence_path(&series_id_text, &original_start_text)?;
    let (event_changes, input_zone) = patch_request.event_changes.into_event_changes()?;
    let changed_occurrence = ChangeSeriesOccurrence::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            &OccurrenceRequest {
                series_id,
                original_start,
                occurrence_scope,
                event_changes: &event_changes,
                input_zone: input_zone.as_ref(),
            },
        )
        .await?;
    Ok(Json(changed_occurrence.into()))
}
