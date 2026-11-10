use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::cards::actions::list_archived_cards::ListArchivedCards;
use crate::features::cards::responses::card_workflow_response::ArchivedCardsResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetArchivedCardsQuery {
    pub before: Option<String>,
}

pub async fn get_archived_cards(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Path(board_id_text): Path<String>,
    Query(archived_query): Query<GetArchivedCardsQuery>,
) -> Result<Json<ArchivedCardsResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    let before_card_id = archived_query
        .before
        .as_deref()
        .map(|before_text| {
            Uuid::parse_str(before_text)
                .map_err(|_| AppError::Validation(String::from("before is a card id")))
        })
        .transpose()?;
    let archived_cards_page = ListArchivedCards::new(&app_state.db_pool)
        .execute(current_session.user_id, board_id, before_card_id)
        .await?;
    Ok(Json(archived_cards_page.into()))
}
