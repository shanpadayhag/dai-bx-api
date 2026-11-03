use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::actions::create_board::CreateBoard;
use crate::features::boards::entities::board::validate_board_name;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostBoardsRequest {
    pub name: String,
}

pub async fn post_boards(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Json(post_request): Json<PostBoardsRequest>,
) -> Result<(StatusCode, Json<BoardResponse>), AppError> {
    let board_name = validate_board_name(&post_request.name)?;
    let created_board = CreateBoard::new()
        .execute(&app_state.db_pool, write_session.user_id, &board_name)
        .await?;
    Ok((StatusCode::CREATED, Json(created_board.into())))
}
