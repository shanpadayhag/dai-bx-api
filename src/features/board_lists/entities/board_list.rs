use uuid::Uuid;

use crate::core::idle_length::IdleLength;

use crate::features::board_lists::error::BoardListError;

pub const LIST_NAME_MAX_CHARACTERS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoardList {
    pub id: Uuid,
    pub user_id: Uuid,
    pub board_id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListExportHeader {
    pub board_name: String,
    pub list_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedValue {
    pub property_name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedCard {
    pub id: Uuid,
    pub title: String,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub values: Vec<ExportedValue>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListExport {
    pub header: ListExportHeader,
    pub cards: Vec<ExportedCard>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPlace {
    pub position: i32,
    pub list_count: i32,
}

pub fn validate_list_name(raw_name: &str) -> Result<String, BoardListError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > LIST_NAME_MAX_CHARACTERS {
        return Err(BoardListError::Invalid(String::from(
            "a list name is 1 to 100 characters",
        )));
    }
    Ok(trimmed_name.to_owned())
}

pub fn validate_list_position(raw_position: i64) -> Result<i32, BoardListError> {
    if raw_position < 0 {
        return Err(BoardListError::Invalid(String::from(
            "a position is 0 or more",
        )));
    }
    Ok(i32::try_from(raw_position).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_a_list_name() {
        assert_eq!(
            validate_list_name(" Backlog  ").expect("a valid name"),
            "Backlog"
        );
    }

    #[test]
    fn refuses_an_empty_or_over_long_list_name() {
        assert!(validate_list_name("   ").is_err());
        assert!(validate_list_name(&"a".repeat(101)).is_err());
        assert!(validate_list_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn refuses_a_negative_position_and_caps_a_huge_one() {
        assert!(validate_list_position(-1).is_err());
        assert_eq!(validate_list_position(0).expect("a position"), 0);
        assert_eq!(
            validate_list_position(i64::MAX).expect("a position"),
            i32::MAX
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListChanges {
    pub name: Option<String>,
    pub archive_after_change: Option<Option<IdleLength>>,
}
