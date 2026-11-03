use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_properties::actions::delete_board_property::DeleteBoardProperty;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_property(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(property_id_text): Path<String>,
) -> Result<Json<BoardResponse>, AppError> {
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let board = DeleteBoardProperty::new(&app_state.db_pool)
        .execute(write_session.user_id, property_id)
        .await?;
    Ok(Json(board.into()))
}
