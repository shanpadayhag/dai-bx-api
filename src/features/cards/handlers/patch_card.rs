use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::update_card::UpdateCard;
use crate::features::cards::entities::card::{
    CardChanges, validate_card_notes_change, validate_card_title, validate_due_date,
    validate_due_time,
};
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchCardRequest {
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub title: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub notes: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub due_time: Option<Option<String>>,
    #[serde(default)]
    pub rename_events: bool,
}

pub async fn patch_card(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
    Json(patch_request): Json<PatchCardRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let card_changes = CardChanges {
        title: patch_request
            .title
            .map(|title| validate_card_title(title.as_deref().unwrap_or_default()))
            .transpose()?,
        notes: patch_request
            .notes
            .map(|notes| validate_card_notes_change(notes.as_deref()))
            .transpose()?,
        due_date_change: patch_request
            .due_date
            .map(|due_date| due_date.as_deref().map(validate_due_date).transpose())
            .transpose()?,
        due_time_change: patch_request
            .due_time
            .map(|due_time| due_time.as_deref().map(validate_due_time).transpose())
            .transpose()?,
    };
    let board = UpdateCard::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            card_id,
            &card_changes,
            patch_request.rename_events,
        )
        .await?;
    Ok(Json(board.into()))
}
