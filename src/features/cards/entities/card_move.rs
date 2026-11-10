use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::error::CardError;

pub const CARD_MOVE_PAGE_LIMIT_DEFAULT: i64 = 50;
const CARD_MOVE_PAGE_LIMIT_MAX: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardMoveReason {
    ByHand,
    Schedule,
    Started,
    Finished,
    ArchivedIdle,
    ArchivedByHand,
    Restored,
}

impl CardMoveReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByHand => "by_hand",
            Self::Schedule => "schedule",
            Self::Started => "started",
            Self::Finished => "finished",
            Self::ArchivedIdle => "archived_idle",
            Self::ArchivedByHand => "archived_by_hand",
            Self::Restored => "restored",
        }
    }

    pub fn from_stored(stored_reason: &str) -> Option<Self> {
        [
            Self::ByHand,
            Self::Schedule,
            Self::Started,
            Self::Finished,
            Self::ArchivedIdle,
            Self::ArchivedByHand,
            Self::Restored,
        ]
        .into_iter()
        .find(|reason| reason.as_str() == stored_reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveCause {
    pub reason: CardMoveReason,
    pub task_title: Option<String>,
    pub moved_at: UtcInstant,
}

impl MoveCause {
    pub fn without_task(reason: CardMoveReason, moved_at: UtcInstant) -> Self {
        Self {
            reason,
            task_title: None,
            moved_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCardMove<'a> {
    pub card_id: Uuid,
    pub from_list_id: Option<Uuid>,
    pub to_list_id: Option<Uuid>,
    pub cause: &'a MoveCause,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardMove {
    pub id: Uuid,
    pub moved_at: UtcInstant,
    pub from_list_name: Option<String>,
    pub to_list_name: Option<String>,
    pub reason: CardMoveReason,
    pub task_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardMovesPage {
    pub moves: Vec<CardMove>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovePage {
    pub before_move_id: Option<Uuid>,
    pub limit: i64,
}

impl MovePage {
    pub fn parse(before_text: Option<&str>, limit_text: Option<&str>) -> Result<Self, CardError> {
        let before_move_id = before_text
            .map(|before_text| {
                Uuid::parse_str(before_text)
                    .map_err(|_| CardError::Invalid(String::from("before is a move id")))
            })
            .transpose()?;
        let limit = limit_text
            .map(|limit_text| {
                limit_text
                    .parse::<i64>()
                    .ok()
                    .filter(|limit| (1..=CARD_MOVE_PAGE_LIMIT_MAX).contains(limit))
                    .ok_or_else(|| CardError::Invalid(String::from("limit is 1 to 50")))
            })
            .transpose()?
            .unwrap_or(CARD_MOVE_PAGE_LIMIT_DEFAULT);
        Ok(Self {
            before_move_id,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_every_stored_reason() {
        for reason in [
            CardMoveReason::ByHand,
            CardMoveReason::Schedule,
            CardMoveReason::Started,
            CardMoveReason::Finished,
            CardMoveReason::ArchivedIdle,
            CardMoveReason::ArchivedByHand,
            CardMoveReason::Restored,
        ] {
            assert_eq!(CardMoveReason::from_stored(reason.as_str()), Some(reason));
        }
        assert_eq!(CardMoveReason::from_stored("teleported"), None);
    }

    #[test]
    fn pages_fifty_moves_by_default_and_refuses_bad_paging() {
        assert_eq!(
            MovePage::parse(None, None).expect("no paging is fine"),
            MovePage {
                before_move_id: None,
                limit: 50
            }
        );
        assert!(MovePage::parse(Some("yesterday"), None).is_err());
        assert!(MovePage::parse(None, Some("0")).is_err());
        assert!(MovePage::parse(None, Some("51")).is_err());
    }
}
