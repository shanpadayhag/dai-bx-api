use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::set_card_value::SetCardValue;
use crate::features::cards::entities::card::validate_card_value;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutCardValueRequest {
    #[serde(default)]
    pub option_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
}

pub async fn put_card_value(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path((card_id_text, property_id_text)): Path<(String, String)>,
    Json(put_request): Json<PutCardValueRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let card_value_content = validate_card_value(
        put_request.option_id.as_deref(),
        put_request.text.as_deref(),
        put_request.number.as_deref(),
    )?;
    let board = SetCardValue::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            card_id,
            property_id,
            &card_value_content,
        )
        .await?;
    Ok(Json(board.into()))
}
