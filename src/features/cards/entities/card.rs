use uuid::Uuid;

use crate::features::boards::entities::board::{BoardCard, BoardPropertyKind, CardValueContent};
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::cards::error::CardError;

pub const CARD_TITLE_MAX_CHARACTERS: usize = 500;
pub const CARD_NOTES_MAX_CHARACTERS: usize = 20_000;
pub const NEW_CARD_NOTES: &str = "## Outcome\n\n## Context\n\n## Open questions\n\n## Done when\n";
pub const CARD_TEXT_VALUE_MAX_CHARACTERS: usize = 500;
pub const CARD_NUMBER_WHOLE_DIGITS_MAX: usize = 15;
pub const CARD_NUMBER_FRACTION_DIGITS_MAX: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCard {
    pub id: Uuid,
    pub user_id: Uuid,
    pub board_id: Uuid,
    pub list_id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedTask {
    pub id: Uuid,
    pub name: String,
    pub is_done: bool,
    pub group_id: Uuid,
    pub group_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardWithNotes {
    pub card: BoardCard,
    pub notes: String,
    pub tracked_event_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDetail {
    pub card: BoardCard,
    pub notes: String,
    pub linked_tasks: Vec<LinkedTask>,
    pub tracked_event_count: i64,
    pub schedules: Vec<CardSchedule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDue {
    pub due_date: Option<String>,
    pub due_time: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardPlace {
    pub list_id: Uuid,
    pub position: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSave {
    Saved,
    UnknownOption,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardChanges {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub due_date_change: Option<Option<String>>,
    pub due_time_change: Option<Option<String>>,
}

impl CardChanges {
    pub fn apply_to_due(&self, current_due: CardDue) -> Result<CardDue, CardError> {
        let changed_due = CardDue {
            due_date: self.due_date_change.clone().unwrap_or(current_due.due_date),
            due_time: self.due_time_change.clone().unwrap_or(current_due.due_time),
        };
        if changed_due.due_time.is_some() && changed_due.due_date.is_none() {
            return Err(CardError::Invalid(String::from(
                "a due time needs a due date",
            )));
        }
        Ok(changed_due)
    }
}

pub fn validate_card_title(raw_title: &str) -> Result<String, CardError> {
    let trimmed_title = raw_title.trim();
    let title_character_count = trimmed_title.chars().count();
    if title_character_count == 0 || title_character_count > CARD_TITLE_MAX_CHARACTERS {
        return Err(CardError::Invalid(String::from(
            "a card title is 1 to 500 characters",
        )));
    }
    Ok(trimmed_title.to_owned())
}

pub fn validate_card_notes(raw_notes: &str) -> Result<String, CardError> {
    if raw_notes.chars().count() > CARD_NOTES_MAX_CHARACTERS {
        return Err(CardError::Invalid(String::from(
            "notes are at most 20,000 characters",
        )));
    }
    Ok(raw_notes.to_owned())
}

pub fn validate_card_notes_change(raw_notes: Option<&str>) -> Result<String, CardError> {
    raw_notes.map(validate_card_notes).unwrap_or_else(|| {
        Err(CardError::Invalid(String::from(
            "notes can't be null; send \"\" to empty them",
        )))
    })
}

pub fn validate_due_date(raw_date: &str) -> Result<String, CardError> {
    let invalid_date =
        || CardError::Invalid(String::from("a due date is a real date like 2026-11-05"));
    let date_bytes = raw_date.as_bytes();
    if !raw_date.is_ascii()
        || date_bytes.len() != 10
        || date_bytes[4] != b'-'
        || date_bytes[7] != b'-'
    {
        return Err(invalid_date());
    }
    let year = parse_digits(&raw_date[0..4]).ok_or_else(invalid_date)?;
    let month = parse_digits(&raw_date[5..7]).ok_or_else(invalid_date)?;
    let day = parse_digits(&raw_date[8..10]).ok_or_else(invalid_date)?;
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return Err(invalid_date());
    }
    Ok(raw_date.to_owned())
}

pub fn validate_due_time(raw_time: &str) -> Result<String, CardError> {
    let invalid_time = || {
        CardError::Invalid(String::from(
            "a due time is like 09:30, from 00:00 to 23:59",
        ))
    };
    let time_bytes = raw_time.as_bytes();
    if !raw_time.is_ascii() || time_bytes.len() != 5 || time_bytes[2] != b':' {
        return Err(invalid_time());
    }
    let hour = parse_digits(&raw_time[0..2]).ok_or_else(invalid_time)?;
    let minute = parse_digits(&raw_time[3..5]).ok_or_else(invalid_time)?;
    if hour > 23 || minute > 59 {
        return Err(invalid_time());
    }
    Ok(raw_time.to_owned())
}

pub fn validate_card_position(raw_position: i64) -> Result<i32, CardError> {
    if raw_position < 0 {
        return Err(CardError::Invalid(String::from("a position is 0 or more")));
    }
    Ok(i32::try_from(raw_position).unwrap_or(i32::MAX))
}

pub fn validate_text_value(raw_text: &str) -> Result<String, CardError> {
    let trimmed_text = raw_text.trim();
    let text_character_count = trimmed_text.chars().count();
    if text_character_count == 0 || text_character_count > CARD_TEXT_VALUE_MAX_CHARACTERS {
        return Err(CardError::Invalid(String::from(
            "a text value is 1 to 500 characters",
        )));
    }
    Ok(trimmed_text.to_owned())
}

fn is_digit_run(digit_text: &str) -> bool {
    !digit_text.is_empty()
        && digit_text
            .bytes()
            .all(|digit_byte| digit_byte.is_ascii_digit())
}

pub fn validate_number_value(raw_number: &str) -> Result<String, CardError> {
    let trimmed_number = raw_number.trim();
    let unsigned_number = trimmed_number.strip_prefix('-').unwrap_or(trimmed_number);
    let (whole_digits, fraction_digits) = match unsigned_number.split_once('.') {
        Some((whole_digits, fraction_digits)) => (whole_digits, Some(fraction_digits)),
        None => (unsigned_number, None),
    };
    let is_number = is_digit_run(whole_digits)
        && whole_digits.trim_start_matches('0').len() <= CARD_NUMBER_WHOLE_DIGITS_MAX
        && fraction_digits.is_none_or(|fraction_digits| {
            is_digit_run(fraction_digits)
                && fraction_digits.len() <= CARD_NUMBER_FRACTION_DIGITS_MAX
        });
    if !is_number {
        return Err(CardError::Invalid(String::from(
            "a number has at most 15 digits before the point and 6 after, like 2.50",
        )));
    }
    Ok(trimmed_number.to_owned())
}

pub fn validate_card_value(
    raw_option_id: Option<&str>,
    raw_text: Option<&str>,
    raw_number: Option<&str>,
) -> Result<CardValueContent, CardError> {
    match (raw_option_id, raw_text, raw_number) {
        (Some(raw_option_id), None, None) => Uuid::parse_str(raw_option_id)
            .map(CardValueContent::Option)
            .map_err(|_| unknown_option_error()),
        (None, Some(raw_text), None) => validate_text_value(raw_text).map(CardValueContent::Text),
        (None, None, Some(raw_number)) => {
            validate_number_value(raw_number).map(CardValueContent::Number)
        }
        _ => Err(CardError::Invalid(String::from(
            "send exactly one of optionId, text or number",
        ))),
    }
}

pub fn unknown_option_error() -> CardError {
    CardError::Invalid(String::from(
        "that option isn't one of this property's options",
    ))
}

pub fn ensure_value_fits_kind(
    card_value_content: &CardValueContent,
    property_kind: BoardPropertyKind,
) -> Result<(), CardError> {
    if card_value_content.kind() == property_kind {
        return Ok(());
    }
    Err(CardError::Invalid(String::from(match property_kind {
        BoardPropertyKind::Choice => "this property takes an optionId",
        BoardPropertyKind::Text => "this property takes text",
        BoardPropertyKind::Number => "this property takes a number",
    })))
}

fn parse_digits(digit_text: &str) -> Option<u32> {
    if digit_text
        .bytes()
        .all(|digit_byte| digit_byte.is_ascii_digit())
    {
        digit_text.parse().ok()
    } else {
        None
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_number_exactly_as_written_within_the_limits() {
        for valid_number in ["2.50", "-0.000001", "0", "999999999999999.999999", "0007.5"] {
            assert_eq!(
                validate_number_value(valid_number).expect("a valid number"),
                valid_number
            );
        }
        assert_eq!(validate_number_value(" 12 ").expect("a valid number"), "12");
    }

    #[test]
    fn refuses_a_number_outside_the_limits_or_not_written_plainly() {
        for bad_number in [
            "",
            "-",
            "1000000000000000",
            "1.0000001",
            ".5",
            "5.",
            "+5",
            "1e3",
            "1,5",
            "--1",
            "１",
        ] {
            assert!(validate_number_value(bad_number).is_err(), "{bad_number}");
        }
    }

    #[test]
    fn trims_text_values_and_refuses_empty_or_over_long_ones() {
        assert_eq!(
            validate_text_value("  https://example.com ").expect("a valid text"),
            "https://example.com"
        );
        assert!(validate_text_value(" \n ").is_err());
        assert!(validate_text_value(&"汉".repeat(500)).is_ok());
        assert!(validate_text_value(&"a".repeat(501)).is_err());
    }

    #[test]
    fn takes_exactly_one_kind_of_value() {
        let option_id = Uuid::now_v7();
        assert_eq!(
            validate_card_value(Some(&option_id.to_string()), None, None).expect("an option"),
            CardValueContent::Option(option_id)
        );
        assert!(validate_card_value(Some("not-a-uuid"), None, None).is_err());
        assert!(validate_card_value(None, None, None).is_err());
        assert!(validate_card_value(None, Some("a"), Some("1")).is_err());
    }

    #[test]
    fn checks_a_value_against_its_property_s_kind() {
        let number_value = CardValueContent::Number(String::from("1"));
        assert!(ensure_value_fits_kind(&number_value, BoardPropertyKind::Number).is_ok());
        assert!(ensure_value_fits_kind(&number_value, BoardPropertyKind::Text).is_err());
        assert!(
            ensure_value_fits_kind(
                &CardValueContent::Text(String::from("a")),
                BoardPropertyKind::Choice
            )
            .is_err()
        );
    }

    #[test]
    fn trims_a_title_and_refuses_an_empty_or_over_long_one() {
        assert_eq!(
            validate_card_title("  Plan ").expect("a valid title"),
            "Plan"
        );
        assert!(validate_card_title(" ").is_err());
        assert!(validate_card_title(&"a".repeat(501)).is_err());
        assert!(validate_card_title(&"é".repeat(500)).is_ok());
    }

    #[test]
    fn keeps_notes_as_written_up_to_20_000_characters() {
        assert_eq!(
            validate_card_notes("  ## Outcome  ").expect("notes"),
            "  ## Outcome  "
        );
        assert!(validate_card_notes(&"é".repeat(20_000)).is_ok());
        assert!(validate_card_notes(&"a".repeat(20_001)).is_err());
        assert_eq!(validate_card_notes_change(Some("")).expect("notes"), "");
        assert!(validate_card_notes_change(None).is_err());
    }

    #[test]
    fn accepts_only_real_dates_in_year_month_day_form() {
        for accepted_date in [
            "2026-11-05",
            "2028-02-29",
            "2000-02-29",
            "0001-01-01",
            "9999-12-31",
        ] {
            assert!(validate_due_date(accepted_date).is_ok(), "{accepted_date}");
        }
        for rejected_date in [
            "2026-02-29",
            "1900-02-29",
            "2026-04-31",
            "2026-13-01",
            "2026-00-10",
            "0000-01-01",
            "2026-1-05",
            "2026/11/05",
            "+026-11-05",
            "2026-11-05T00:00",
            "2é26-11-05",
            "2026-11-é",
        ] {
            assert!(validate_due_date(rejected_date).is_err(), "{rejected_date}");
        }
    }

    #[test]
    fn accepts_only_hours_and_minutes_on_a_24_hour_clock() {
        for accepted_time in ["00:00", "09:30", "23:59"] {
            assert!(validate_due_time(accepted_time).is_ok(), "{accepted_time}");
        }
        for rejected_time in [
            "24:00", "12:60", "9:30", "09:30:00", "0a:30", "-1:30", "é:30", "09:é",
        ] {
            assert!(validate_due_time(rejected_time).is_err(), "{rejected_time}");
        }
    }

    #[test]
    fn applies_due_changes_and_refuses_a_time_left_without_a_date() {
        let current_due = CardDue {
            due_date: Some(String::from("2026-11-05")),
            due_time: Some(String::from("18:00")),
        };
        let clear_time = CardChanges {
            due_time_change: Some(None),
            ..CardChanges::default()
        };
        let clear_date_only = CardChanges {
            due_date_change: Some(None),
            ..CardChanges::default()
        };

        assert_eq!(
            clear_time.apply_to_due(current_due.clone()).expect("a due"),
            CardDue {
                due_date: Some(String::from("2026-11-05")),
                due_time: None,
            }
        );
        assert_eq!(
            CardChanges::default()
                .apply_to_due(current_due.clone())
                .expect("a due"),
            current_due
        );
        assert!(clear_date_only.apply_to_due(current_due).is_err());
    }
}
