use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::clear_card_value::ClearCardValue;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_card_value(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path((card_id_text, property_id_text)): Path<(String, String)>,
) -> Result<Json<BoardResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let board = ClearCardValue::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            card_id,
            property_id,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(board.into()))
}
