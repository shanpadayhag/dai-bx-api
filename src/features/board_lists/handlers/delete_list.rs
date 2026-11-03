use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_lists::actions::delete_board_list::DeleteBoardList;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_list(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(list_id_text): Path<String>,
) -> Result<Json<BoardResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let board = DeleteBoardList::new(&app_state.db_pool)
        .execute(write_session.user_id, list_id)
        .await?;
    Ok(Json(board.into()))
}
