use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_lists::actions::create_board_list::CreateBoardList;
use crate::features::board_lists::entities::board_list::validate_list_name;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostBoardListsRequest {
    pub name: String,
}

pub async fn post_board_lists(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(board_id_text): Path<String>,
    Json(post_request): Json<PostBoardListsRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    let list_name = validate_list_name(&post_request.name)?;
    let board = CreateBoardList::new(&app_state.db_pool)
        .execute(write_session.user_id, board_id, &list_name)
        .await?;
    Ok(Json(board.into()))
}
