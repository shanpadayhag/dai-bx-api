use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::delete_calendar_event::DeleteCalendarEvent;
use crate::features::calendar_events::responses::deleted_response::DeletedResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_calendar_event(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(event_id_text): Path<String>,
) -> Result<Json<DeletedResponse>, AppError> {
    let event_id = Uuid::parse_str(&event_id_text).map_err(|_| AppError::NotFound)?;
    DeleteCalendarEvent::new(&app_state.db_pool)
        .execute(write_session.user_id, event_id)
        .await?;
    Ok(Json(DeletedResponse::deleted()))
}
