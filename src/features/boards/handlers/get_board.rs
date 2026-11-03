use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::actions::find_board::FindBoard;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_board(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Path(board_id_text): Path<String>,
) -> Result<Json<BoardResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    let board = FindBoard::new(&app_state.db_pool)
        .execute(current_session.user_id, board_id)
        .await?;
    Ok(Json(board.into()))
}
