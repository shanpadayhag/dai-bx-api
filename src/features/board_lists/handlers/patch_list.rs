use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::idle_length::{IDLE_LENGTH_HINT, IdleLength};
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::board_lists::actions::update_board_list::UpdateBoardList;
use crate::features::board_lists::entities::board_list::{ListChanges, validate_list_name};
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchListRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub archive_after: Option<Option<String>>,
}

fn parse_archive_after(archive_after_text: Option<&str>) -> Result<Option<IdleLength>, AppError> {
    archive_after_text
        .map(|archive_after_text| {
            IdleLength::parse(archive_after_text)
                .ok_or_else(|| AppError::Validation(String::from(IDLE_LENGTH_HINT)))
        })
        .transpose()
}

pub async fn patch_list(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(list_id_text): Path<String>,
    Json(patch_request): Json<PatchListRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let list_changes = ListChanges {
        name: patch_request
            .name
            .as_deref()
            .map(validate_list_name)
            .transpose()?,
        archive_after_change: patch_request
            .archive_after
            .map(|archive_after_text| parse_archive_after(archive_after_text.as_deref()))
            .transpose()?,
    };
    let board = UpdateBoardList::new(&app_state.db_pool)
        .execute(write_session.user_id, list_id, list_changes)
        .await?;
    Ok(Json(board.into()))
}
