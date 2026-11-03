use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_lists::actions::rename_board_list::RenameBoardList;
use crate::features::board_lists::entities::board_list::validate_list_name;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchListRequest {
    pub name: String,
}

pub async fn patch_list(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(list_id_text): Path<String>,
    Json(patch_request): Json<PatchListRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let list_name = validate_list_name(&patch_request.name)?;
    let board = RenameBoardList::new(&app_state.db_pool)
        .execute(write_session.user_id, list_id, &list_name)
        .await?;
    Ok(Json(board.into()))
}
