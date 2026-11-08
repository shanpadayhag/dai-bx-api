use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::delete_board::DeleteBoard;
use crate::features::boards::responses::deleted_response::DeletedResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_board(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(board_id_text): Path<String>,
) -> Result<Json<DeletedResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    DeleteBoard::new(&app_state.db_pool)
        .execute(write_session.user_id, board_id, UtcInstant::now())
        .await?;
    Ok(Json(DeletedResponse::deleted()))
}
