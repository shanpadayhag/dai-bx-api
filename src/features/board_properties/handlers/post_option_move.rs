use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_properties::actions::move_property_option::MovePropertyOption;
use crate::features::board_properties::entities::board_property::validate_position;
use crate::features::board_properties::handlers::post_property_move::PostMoveRequest;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn post_option_move(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(option_id_text): Path<String>,
    Json(move_request): Json<PostMoveRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let option_id = Uuid::parse_str(&option_id_text).map_err(|_| AppError::NotFound)?;
    let requested_position = validate_position(move_request.position)?;
    let board = MovePropertyOption::new(&app_state.db_pool)
        .execute(write_session.user_id, option_id, requested_position)
        .await?;
    Ok(Json(board.into()))
}
