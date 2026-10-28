use uuid::Uuid;

use crate::features::categories::error::CategoryError;

pub const CATEGORY_NAME_MAX_CHARACTERS: usize = 50;

pub const DEFAULT_CATEGORIES: [(&str, &str); 4] = [
    ("Goals", "#7c95bd"),
    ("Personal", "#3a405a"),
    ("Studies", "#be7e86"),
    ("Work", "#c6afa3"),
];

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCategory {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryChanges {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategorySave {
    Saved(Category),
    NameTaken,
}

pub fn validate_category_name(raw_name: &str) -> Result<String, CategoryError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > CATEGORY_NAME_MAX_CHARACTERS {
        return Err(CategoryError::Invalid(String::from(
            "a category name is 1 to 50 characters",
        )));
    }
    Ok(trimmed_name.to_owned())
}

pub fn validate_category_color(raw_color: &str) -> Result<String, CategoryError> {
    let is_hex_color = raw_color.len() == 7
        && raw_color.starts_with('#')
        && raw_color[1..]
            .chars()
            .all(|color_digit| color_digit.is_ascii_hexdigit());
    if !is_hex_color {
        return Err(CategoryError::Invalid(String::from(
            "a color is # followed by six hex digits, like #8fae9a",
        )));
    }
    Ok(raw_color.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_a_category_name() {
        assert_eq!(
            validate_category_name("  Health  ").expect("a valid name"),
            "Health"
        );
    }

    #[test]
    fn counts_characters_not_bytes_in_a_category_name() {
        assert!(validate_category_name(&"汉".repeat(50)).is_ok());
        assert!(validate_category_name(&"汉".repeat(51)).is_err());
    }

    #[test]
    fn refuses_an_empty_category_name() {
        assert!(validate_category_name("   ").is_err());
    }

    #[test]
    fn lowercases_a_hex_color() {
        assert_eq!(
            validate_category_color("#8FAE9A").expect("a valid color"),
            "#8fae9a"
        );
    }

    #[test]
    fn refuses_anything_but_hash_and_six_hex_digits() {
        for rejected_color in [
            "red",
            "#12345",
            "#12345g",
            "123456",
            "#1234567",
            "#１２３４５",
        ] {
            assert!(
                validate_category_color(rejected_color).is_err(),
                "{rejected_color} should be refused"
            );
        }
    }
}
