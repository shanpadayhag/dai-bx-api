use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::cards::actions::create_card::CreateCard;
use crate::features::cards::entities::card::validate_card_title;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostListCardsRequest {
    pub title: String,
}

pub async fn post_list_cards(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(list_id_text): Path<String>,
    Json(post_request): Json<PostListCardsRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let card_title = validate_card_title(&post_request.title)?;
    let board = CreateCard::new(&app_state.db_pool)
        .execute(write_session.user_id, list_id, &card_title)
        .await?;
    Ok(Json(board.into()))
}
