use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::cards::actions::save_card_triggers::SaveCardTriggers;
use crate::features::cards::entities::card_trigger::TriggerChoice;
use crate::features::cards::responses::card_workflow_response::SavedTriggersResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutCardTriggersRequest {
    #[serde(default)]
    pub started_list_id: Option<String>,
    #[serde(default)]
    pub finished_list_id: Option<String>,
}

fn parse_list_id(list_id_text: Option<&str>) -> Result<Option<Uuid>, AppError> {
    list_id_text
        .map(|list_id_text| Uuid::parse_str(list_id_text).map_err(|_| AppError::NotFound))
        .transpose()
}

pub async fn put_card_triggers(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
    Json(triggers_request): Json<PutCardTriggersRequest>,
) -> Result<Json<SavedTriggersResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let trigger_choice = TriggerChoice {
        started_list_id: parse_list_id(triggers_request.started_list_id.as_deref())?,
        finished_list_id: parse_list_id(triggers_request.finished_list_id.as_deref())?,
    };
    let card_triggers = SaveCardTriggers::new(&app_state.db_pool)
        .execute(write_session.user_id, card_id, trigger_choice)
        .await?;
    Ok(Json(SavedTriggersResponse {
        triggers: card_triggers.into(),
    }))
}
