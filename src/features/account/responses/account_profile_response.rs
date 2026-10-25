use serde::Serialize;

use crate::features::account::entities::account_profile::{AccentColor, AccountProfile, ThemeMode};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfileResponse {
    pub name: String,
    pub email: String,
    pub accent_color: AccentColor,
    pub theme: ThemeMode,
}

impl From<AccountProfile> for AccountProfileResponse {
    fn from(account_profile: AccountProfile) -> Self {
        Self {
            name: account_profile.name,
            email: account_profile.email,
            accent_color: account_profile.accent_color,
            theme: account_profile.theme,
        }
    }
}
