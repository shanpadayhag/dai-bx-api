use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::update_board::UpdateBoard;
use crate::features::boards::entities::board::validate_board_name;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchBoardRequest {
    pub name: String,
}

pub async fn patch_board(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(board_id_text): Path<String>,
    Json(patch_request): Json<PatchBoardRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    let board_name = validate_board_name(&patch_request.name)?;
    let updated_board = UpdateBoard::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            board_id,
            &board_name,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(updated_board.into()))
}
