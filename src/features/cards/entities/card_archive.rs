use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::entities::card_move::CardMoveReason;

pub const ARCHIVED_CARD_PAGE_LIMIT: i64 = 50;
pub const IDLE_CARDS_PER_CALL: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveHow {
    Idle,
    ByHand,
}

impl ArchiveHow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::ByHand => "by_hand",
        }
    }

    pub fn from_stored(stored_how: &str) -> Option<Self> {
        match stored_how {
            "idle" => Some(Self::Idle),
            "by_hand" => Some(Self::ByHand),
            _ => None,
        }
    }

    pub fn move_reason(self) -> CardMoveReason {
        match self {
            Self::Idle => CardMoveReason::ArchivedIdle,
            Self::ByHand => CardMoveReason::ArchivedByHand,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardArchive {
    pub archived_at: UtcInstant,
    pub how: ArchiveHow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedCard {
    pub id: Uuid,
    pub title: String,
    pub from_list_name: Option<String>,
    pub archive: CardArchive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedCardsPage {
    pub cards: Vec<ArchivedCard>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleCard {
    pub card_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IdleArchiveTally {
    pub archived_count: usize,
    pub failed_count: usize,
}
