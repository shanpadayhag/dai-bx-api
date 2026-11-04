use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::check_in_running_event::CheckInRunningEvent;
use crate::features::throttling::entities::write_session::WriteSession;
use crate::features::tracking::responses::running_item_response::RunningItemResponse;

pub async fn post_check_in(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(event_id_text): Path<String>,
) -> Result<Json<RunningItemResponse>, AppError> {
    let event_id = Uuid::parse_str(&event_id_text).map_err(|_| AppError::NotFound)?;
    let checked_in_item = CheckInRunningEvent::new(&app_state.db_pool)
        .execute(write_session.user_id, event_id)
        .await?;
    Ok(Json(checked_in_item.into()))
}
