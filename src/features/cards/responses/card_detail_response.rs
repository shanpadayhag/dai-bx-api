use serde::Serialize;

use crate::features::boards::responses::board_response::BoardCardResponse;
use crate::features::cards::entities::card::{CardDetail, LinkedTask};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedTaskResponse {
    pub id: String,
    pub name: String,
    pub is_done: bool,
    pub group_id: String,
    pub group_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDetailResponse {
    pub card: BoardCardResponse,
    pub notes: String,
    pub linked_tasks: Vec<LinkedTaskResponse>,
    pub tracked_event_count: i64,
}

impl From<LinkedTask> for LinkedTaskResponse {
    fn from(linked_task: LinkedTask) -> Self {
        Self {
            id: linked_task.id.to_string(),
            name: linked_task.name,
            is_done: linked_task.is_done,
            group_id: linked_task.group_id.to_string(),
            group_name: linked_task.group_name,
        }
    }
}

impl From<CardDetail> for CardDetailResponse {
    fn from(card_detail: CardDetail) -> Self {
        Self {
            card: card_detail.card.into(),
            notes: card_detail.notes,
            linked_tasks: card_detail
                .linked_tasks
                .into_iter()
                .map(LinkedTaskResponse::from)
                .collect(),
            tracked_event_count: card_detail.tracked_event_count,
        }
    }
}
