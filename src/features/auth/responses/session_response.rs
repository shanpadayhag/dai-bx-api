use serde::Serialize;

use crate::features::account::entities::account_profile::{AccentColor, AccountProfile, ThemeMode};

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub user: SessionUserResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUserResponse {
    pub name: String,
    pub email: String,
    pub accent_color: AccentColor,
    pub theme: ThemeMode,
}

impl From<AccountProfile> for SessionResponse {
    fn from(account_profile: AccountProfile) -> Self {
        Self {
            user: SessionUserResponse {
                name: account_profile.name,
                email: account_profile.email,
                accent_color: account_profile.accent_color,
                theme: account_profile.theme,
            },
        }
    }
}
