use uuid::Uuid;

use crate::features::board_properties::error::BoardPropertyError;
use crate::features::boards::entities::board::BoardPropertyKind;

pub const PROPERTY_NAME_MAX_CHARACTERS: usize = 50;
pub const OPTION_NAME_MAX_CHARACTERS: usize = 50;
pub const BOARD_PROPERTY_LIMIT: i32 = 20;
pub const PROPERTY_OPTION_LIMIT: i32 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBoardProperty {
    pub id: Uuid,
    pub user_id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    pub kind: BoardPropertyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionDraft {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPropertyOption {
    pub id: Uuid,
    pub user_id: Uuid,
    pub property_id: Uuid,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionChanges {
    pub name: Option<String>,
    pub color_change: Option<Option<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyPlace {
    pub position: i32,
    pub property_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertyOptionCount {
    pub kind: BoardPropertyKind,
    pub option_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionPlace {
    pub property_id: Uuid,
    pub position: i32,
    pub option_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameSave {
    Saved,
    Missing,
    NameTaken,
}

fn validate_name(
    raw_name: &str,
    max_characters: usize,
    invalid_message: &str,
) -> Result<String, BoardPropertyError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > max_characters {
        return Err(BoardPropertyError::Invalid(invalid_message.to_owned()));
    }
    Ok(trimmed_name.to_owned())
}

pub fn validate_property_name(raw_name: &str) -> Result<String, BoardPropertyError> {
    validate_name(
        raw_name,
        PROPERTY_NAME_MAX_CHARACTERS,
        "a property name is 1 to 50 characters",
    )
}

pub fn validate_option_name(raw_name: &str) -> Result<String, BoardPropertyError> {
    validate_name(
        raw_name,
        OPTION_NAME_MAX_CHARACTERS,
        "an option name is 1 to 50 characters",
    )
}

pub fn validate_option_color(raw_color: &str) -> Result<String, BoardPropertyError> {
    let is_hex_color = raw_color.len() == 7
        && raw_color.starts_with('#')
        && raw_color[1..]
            .chars()
            .all(|color_digit| color_digit.is_ascii_hexdigit());
    if !is_hex_color {
        return Err(BoardPropertyError::Invalid(String::from(
            "a color is # followed by six hex digits, like #8fae9a",
        )));
    }
    Ok(raw_color.to_ascii_lowercase())
}

pub fn validate_option_draft(
    raw_name: &str,
    raw_color: Option<&str>,
) -> Result<OptionDraft, BoardPropertyError> {
    Ok(OptionDraft {
        name: validate_option_name(raw_name)?,
        color: raw_color.map(validate_option_color).transpose()?,
    })
}

pub fn validate_new_property_options(
    kind: BoardPropertyKind,
    option_drafts: &[OptionDraft],
) -> Result<(), BoardPropertyError> {
    if kind != BoardPropertyKind::Choice {
        if option_drafts.is_empty() {
            return Ok(());
        }
        return Err(BoardPropertyError::Invalid(String::from(
            "only a choice property has options",
        )));
    }
    if option_drafts.is_empty() {
        return Err(BoardPropertyError::Invalid(String::from(
            "a choice property needs at least one option",
        )));
    }
    if option_drafts.len() > PROPERTY_OPTION_LIMIT as usize {
        return Err(BoardPropertyError::Invalid(String::from(
            "a choice property has at most 50 options",
        )));
    }
    Ok(())
}

pub fn validate_position(raw_position: i64) -> Result<i32, BoardPropertyError> {
    if raw_position < 0 {
        return Err(BoardPropertyError::Invalid(String::from(
            "a position is 0 or more",
        )));
    }
    Ok(i32::try_from(raw_position).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option_drafts(option_count: usize) -> Vec<OptionDraft> {
        (0..option_count)
            .map(|option_number| OptionDraft {
                name: format!("Option {option_number}"),
                color: None,
            })
            .collect()
    }

    #[test]
    fn trims_names_and_counts_characters_not_bytes() {
        assert_eq!(
            validate_property_name("  Project ").expect("a valid name"),
            "Project"
        );
        assert!(validate_property_name(&"汉".repeat(50)).is_ok());
        assert!(validate_option_name(&"汉".repeat(50)).is_ok());
    }

    #[test]
    fn refuses_an_empty_or_over_long_name() {
        assert!(validate_property_name("   ").is_err());
        assert!(validate_property_name(&"a".repeat(51)).is_err());
        assert!(validate_option_name("").is_err());
        assert!(validate_option_name(&"a".repeat(51)).is_err());
    }

    #[test]
    fn lowercases_a_color_and_refuses_anything_but_six_hex_digits() {
        assert_eq!(
            validate_option_color("#22C55E").expect("a valid color"),
            "#22c55e"
        );
        for bad_color in ["22c55e", "#22c55", "#22c55e0", "#22c55g", "#汉汉"] {
            assert!(validate_option_color(bad_color).is_err(), "{bad_color}");
        }
    }

    #[test]
    fn keeps_a_missing_color_missing() {
        assert_eq!(
            validate_option_draft(" Small ", None).expect("a valid option"),
            OptionDraft {
                name: String::from("Small"),
                color: None,
            }
        );
    }

    #[test]
    fn needs_one_to_fifty_options_on_a_choice_and_none_elsewhere() {
        assert!(validate_new_property_options(BoardPropertyKind::Choice, &[]).is_err());
        assert!(
            validate_new_property_options(BoardPropertyKind::Choice, &option_drafts(50)).is_ok()
        );
        assert!(
            validate_new_property_options(BoardPropertyKind::Choice, &option_drafts(51)).is_err()
        );
        assert!(validate_new_property_options(BoardPropertyKind::Text, &[]).is_ok());
        assert!(
            validate_new_property_options(BoardPropertyKind::Number, &option_drafts(1)).is_err()
        );
    }

    #[test]
    fn refuses_a_negative_position_and_caps_a_huge_one() {
        assert!(validate_position(-1).is_err());
        assert_eq!(validate_position(3).expect("a position"), 3);
        assert_eq!(validate_position(i64::MAX).expect("a position"), i32::MAX);
    }
}
