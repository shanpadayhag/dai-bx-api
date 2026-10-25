use axum::Json;
use axum::extract::State;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::account::actions::update_account_profile::UpdateAccountProfile;
use crate::features::account::entities::account_profile::{
    ACCENT_COLOR_NAMES, AccentColor, ProfileChanges, THEME_MODE_NAMES, ThemeMode,
};
use crate::features::account::responses::account_profile_response::AccountProfileResponse;
use crate::features::auth::handlers::post_sign_up::validate_name;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchAccountRequest {
    pub name: Option<String>,
    pub accent_color: Option<String>,
    pub theme: Option<String>,
}

pub async fn patch_account(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Json(patch_request): Json<PatchAccountRequest>,
) -> Result<Json<AccountProfileResponse>, AppError> {
    let profile_changes = ProfileChanges {
        name: patch_request
            .name
            .as_deref()
            .map(validate_name)
            .transpose()?
            .map(str::to_owned),
        accent_color: patch_request
            .accent_color
            .as_deref()
            .map(|accent_color_name| {
                AccentColor::parse(accent_color_name).ok_or_else(|| {
                    AppError::Validation(format!("accent color must be {ACCENT_COLOR_NAMES}"))
                })
            })
            .transpose()?,
        theme: patch_request
            .theme
            .as_deref()
            .map(|theme_mode_name| {
                ThemeMode::parse(theme_mode_name).ok_or_else(|| {
                    AppError::Validation(format!("theme must be {THEME_MODE_NAMES}"))
                })
            })
            .transpose()?,
    };
    let account_profile = UpdateAccountProfile::new()
        .execute(
            &app_state.db_pool,
            current_session.user_id,
            &profile_changes,
        )
        .await?;
    Ok(Json(account_profile.into()))
}
