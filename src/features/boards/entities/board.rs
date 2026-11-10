use uuid::Uuid;

use crate::core::idle_length::IdleLength;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::error::BoardError;

pub const BOARD_NAME_MAX_CHARACTERS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardSummary {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoard {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardList {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub archive_after: Option<IdleLength>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardCard {
    pub id: Uuid,
    pub list_id: Option<Uuid>,
    pub position: Option<i32>,
    pub title: String,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub values: Vec<CardValue>,
    pub linked_task_count: i64,
    pub done_linked_task_count: i64,
    pub next_move: Option<NextMove>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextMove {
    pub at: UtcInstant,
    pub list_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardValue {
    pub property_id: Uuid,
    pub content: CardValueContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardValueContent {
    Option(Uuid),
    Text(String),
    Number(String),
}

impl CardValueContent {
    pub fn kind(&self) -> BoardPropertyKind {
        match self {
            Self::Option(_) => BoardPropertyKind::Choice,
            Self::Text(_) => BoardPropertyKind::Text,
            Self::Number(_) => BoardPropertyKind::Number,
        }
    }

    pub fn from_columns(
        option_id: Option<Uuid>,
        text_value: Option<String>,
        number_value: Option<String>,
    ) -> Option<Self> {
        match (option_id, text_value, number_value) {
            (Some(option_id), _, _) => Some(Self::Option(option_id)),
            (None, Some(text_value), _) => Some(Self::Text(text_value)),
            (None, None, Some(number_value)) => Some(Self::Number(number_value)),
            (None, None, None) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum BoardPropertyKind {
    Choice,
    Text,
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardProperty {
    pub id: Uuid,
    pub name: String,
    pub kind: BoardPropertyKind,
    pub position: i32,
    pub options: Vec<PropertyOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyOption {
    pub id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub board: BoardSummary,
    pub lists: Vec<BoardList>,
    pub cards: Vec<BoardCard>,
    pub properties: Vec<BoardProperty>,
}

pub fn validate_board_name(raw_name: &str) -> Result<String, BoardError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > BOARD_NAME_MAX_CHARACTERS {
        return Err(BoardError::Invalid(String::from(
            "a board name is 1 to 100 characters",
        )));
    }
    Ok(trimmed_name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_a_board_name() {
        assert_eq!(
            validate_board_name("  Personal ").expect("a valid name"),
            "Personal"
        );
    }

    #[test]
    fn refuses_an_empty_or_over_long_board_name() {
        assert!(validate_board_name(" ").is_err());
        assert!(validate_board_name(&"a".repeat(101)).is_err());
        assert!(validate_board_name(&"é".repeat(100)).is_ok());
    }
}
