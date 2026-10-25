use serde::Serialize;

pub const ACCENT_COLOR_NAMES: &str = "blue, indigo, teal, violet or graphite";
pub const THEME_MODE_NAMES: &str = "light, dark or system";
pub const ACCOUNT_DELETION_CONFIRMATION: &str = "delete my account";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "accent_color", rename_all = "lowercase")]
pub enum AccentColor {
    Blue,
    Indigo,
    Teal,
    Violet,
    Graphite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "theme_mode", rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl AccentColor {
    pub fn parse(accent_color_name: &str) -> Option<Self> {
        match accent_color_name {
            "blue" => Some(AccentColor::Blue),
            "indigo" => Some(AccentColor::Indigo),
            "teal" => Some(AccentColor::Teal),
            "violet" => Some(AccentColor::Violet),
            "graphite" => Some(AccentColor::Graphite),
            _ => None,
        }
    }
}

impl ThemeMode {
    pub fn parse(theme_mode_name: &str) -> Option<Self> {
        match theme_mode_name {
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            "system" => Some(ThemeMode::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AccountProfile {
    pub name: String,
    pub email: String,
    pub accent_color: AccentColor,
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileChanges {
    pub name: Option<String>,
    pub accent_color: Option<AccentColor>,
    pub theme: Option<ThemeMode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_listed_accent_colors_and_themes() {
        assert_eq!(AccentColor::parse("teal"), Some(AccentColor::Teal));
        assert_eq!(AccentColor::parse("Teal"), None);
        assert_eq!(AccentColor::parse("pink"), None);
        assert_eq!(ThemeMode::parse("dark"), Some(ThemeMode::Dark));
        assert_eq!(ThemeMode::parse("neon"), None);
    }
}
