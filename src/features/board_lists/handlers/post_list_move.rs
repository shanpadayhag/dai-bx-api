use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_lists::actions::move_board_list::MoveBoardList;
use crate::features::board_lists::entities::board_list::validate_list_position;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostListMoveRequest {
    pub position: i64,
}

pub async fn post_list_move(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(list_id_text): Path<String>,
    Json(move_request): Json<PostListMoveRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let requested_position = validate_list_position(move_request.position)?;
    let board = MoveBoardList::new(&app_state.db_pool)
        .execute(write_session.user_id, list_id, requested_position)
        .await?;
    Ok(Json(board.into()))
}
