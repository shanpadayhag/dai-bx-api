use serde::Serialize;

use crate::features::account::entities::account_profile::{AccentColor, AccountProfile, ThemeMode};
use crate::features::account::entities::account_security::AccountSecurity;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
    pub name: String,
    pub email: String,
    pub accent_color: AccentColor,
    pub theme: ThemeMode,
    pub security: AccountSecurityResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSecurityResponse {
    pub recovery_codes_remaining: i64,
    pub active_session_count: i64,
}

impl AccountResponse {
    pub fn new(account_profile: AccountProfile, account_security: AccountSecurity) -> Self {
        Self {
            name: account_profile.name,
            email: account_profile.email,
            accent_color: account_profile.accent_color,
            theme: account_profile.theme,
            security: AccountSecurityResponse {
                recovery_codes_remaining: account_security.recovery_codes_remaining,
                active_session_count: account_security.active_session_count,
            },
        }
    }
}
