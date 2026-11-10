use serde::Serialize;

use crate::features::cards::entities::card_archive::{
    ArchiveHow, ArchivedCard, ArchivedCardsPage, CardArchive,
};
use crate::features::cards::entities::card_move::{CardMove, CardMoveReason, CardMovesPage};
use crate::features::cards::entities::card_trigger::{CardTriggers, TriggerTarget};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerTargetResponse {
    pub list_id: Option<String>,
    pub list_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CardTriggersResponse {
    pub started: Option<TriggerTargetResponse>,
    pub finished: Option<TriggerTargetResponse>,
}

#[derive(Debug, Serialize)]
pub struct SavedTriggersResponse {
    pub triggers: CardTriggersResponse,
}

#[derive(Debug, Serialize)]
pub struct CardArchiveResponse {
    pub at: String,
    pub how: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardMoveResponse {
    pub id: String,
    pub moved_at: String,
    pub from_list_name: Option<String>,
    pub to_list_name: Option<String>,
    pub reason: &'static str,
    pub task_title: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardHistoryResponse {
    pub moves: Vec<CardMoveResponse>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedCardResponse {
    pub id: String,
    pub title: String,
    pub from_list_name: Option<String>,
    pub archived_at: String,
    pub how: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedCardsResponse {
    pub cards: Vec<ArchivedCardResponse>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct ArchivedStatusResponse {
    pub status: &'static str,
}

fn archive_how_text(archive_how: ArchiveHow) -> &'static str {
    match archive_how {
        ArchiveHow::Idle => "idle",
        ArchiveHow::ByHand => "byHand",
    }
}

fn move_reason_text(move_reason: CardMoveReason) -> &'static str {
    match move_reason {
        CardMoveReason::ByHand => "byHand",
        CardMoveReason::Schedule => "schedule",
        CardMoveReason::Started => "started",
        CardMoveReason::Finished => "finished",
        CardMoveReason::ArchivedIdle => "archivedIdle",
        CardMoveReason::ArchivedByHand => "archivedByHand",
        CardMoveReason::Restored => "restored",
    }
}

impl From<TriggerTarget> for TriggerTargetResponse {
    fn from(trigger_target: TriggerTarget) -> Self {
        Self {
            list_id: trigger_target.list_id.map(|list_id| list_id.to_string()),
            list_name: trigger_target.list_name,
        }
    }
}

impl From<CardTriggers> for CardTriggersResponse {
    fn from(card_triggers: CardTriggers) -> Self {
        Self {
            started: card_triggers.started.map(TriggerTargetResponse::from),
            finished: card_triggers.finished.map(TriggerTargetResponse::from),
        }
    }
}

impl From<CardArchive> for CardArchiveResponse {
    fn from(card_archive: CardArchive) -> Self {
        Self {
            at: card_archive.archived_at.to_iso_text(),
            how: archive_how_text(card_archive.how),
        }
    }
}

impl From<CardMove> for CardMoveResponse {
    fn from(card_move: CardMove) -> Self {
        Self {
            id: card_move.id.to_string(),
            moved_at: card_move.moved_at.to_iso_text(),
            from_list_name: card_move.from_list_name,
            to_list_name: card_move.to_list_name,
            reason: move_reason_text(card_move.reason),
            task_title: card_move.task_title,
        }
    }
}

impl From<CardMovesPage> for CardHistoryResponse {
    fn from(card_moves_page: CardMovesPage) -> Self {
        Self {
            moves: card_moves_page
                .moves
                .into_iter()
                .map(CardMoveResponse::from)
                .collect(),
            has_more: card_moves_page.has_more,
        }
    }
}

impl From<ArchivedCard> for ArchivedCardResponse {
    fn from(archived_card: ArchivedCard) -> Self {
        Self {
            id: archived_card.id.to_string(),
            title: archived_card.title,
            from_list_name: archived_card.from_list_name,
            archived_at: archived_card.archive.archived_at.to_iso_text(),
            how: archive_how_text(archived_card.archive.how),
        }
    }
}

impl From<ArchivedCardsPage> for ArchivedCardsResponse {
    fn from(archived_cards_page: ArchivedCardsPage) -> Self {
        Self {
            cards: archived_cards_page
                .cards
                .into_iter()
                .map(ArchivedCardResponse::from)
                .collect(),
            has_more: archived_cards_page.has_more,
        }
    }
}
