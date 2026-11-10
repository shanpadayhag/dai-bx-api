use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::cards::actions::find_card_history::FindCardHistory;
use crate::features::cards::entities::card_move::MovePage;
use crate::features::cards::responses::card_workflow_response::CardHistoryResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetCardHistoryQuery {
    pub before: Option<String>,
    pub limit: Option<String>,
}

pub async fn get_card_history(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Path(card_id_text): Path<String>,
    Query(history_query): Query<GetCardHistoryQuery>,
) -> Result<Json<CardHistoryResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let move_page = MovePage::parse(
        history_query.before.as_deref(),
        history_query.limit.as_deref(),
    )?;
    let card_moves_page = FindCardHistory::new(&app_state.db_pool)
        .execute(current_session.user_id, card_id, move_page)
        .await?;
    Ok(Json(card_moves_page.into()))
}
