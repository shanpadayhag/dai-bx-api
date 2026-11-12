use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::move_card::MoveCard;
use crate::features::cards::entities::card::{CardPlace, validate_card_position};
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostCardMoveRequest {
    pub list_id: String,
    pub position: i64,
}

fn parse_id(id_text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id_text).map_err(|_| AppError::NotFound)
}

pub async fn post_card_move(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(card_id_text): Path<String>,
    Json(move_request): Json<PostCardMoveRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let card_id = parse_id(&card_id_text)?;
    let requested_place = CardPlace {
        list_id: parse_id(&move_request.list_id)?,
        position: validate_card_position(move_request.position)?,
    };
    let board = MoveCard::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            card_id,
            requested_place,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(board.into()))
}
