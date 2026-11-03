use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::delete_card::DeleteCard;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_card(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
) -> Result<Json<BoardResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let board = DeleteCard::new(&app_state.db_pool)
        .execute(write_session.user_id, card_id)
        .await?;
    Ok(Json(board.into()))
}
