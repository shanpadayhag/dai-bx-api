use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::entities::calendar_event::parse_event_instant;
use crate::features::throttling::entities::write_session::WriteSession;
use crate::features::tracking::actions::finish_running_item::FinishRunningItem;
use crate::features::tracking::responses::finished_item_response::FinishedItemResponse;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostFinishRequest {
    pub ends_at: Option<String>,
    pub mark_done: Option<bool>,
}

pub async fn post_finish(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(event_id_text): Path<String>,
    Json(finish_request): Json<PostFinishRequest>,
) -> Result<Json<FinishedItemResponse>, AppError> {
    let event_id = Uuid::parse_str(&event_id_text).map_err(|_| AppError::NotFound)?;
    let requested_ends_at = parse_event_instant(
        finish_request.ends_at.as_deref().unwrap_or_default(),
        "endsAt",
    )?;
    let finished_item = FinishRunningItem::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            event_id,
            requested_ends_at,
            finish_request.mark_done.unwrap_or(false),
        )
        .await?;
    Ok(Json(finished_item.into()))
}
