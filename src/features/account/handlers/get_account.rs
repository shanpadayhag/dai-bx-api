use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::account::actions::find_account_profile::FindAccountProfile;
use crate::features::account::actions::find_account_security::FindAccountSecurity;
use crate::features::account::responses::account_response::AccountResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_account(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<AccountResponse>, AppError> {
    let find_account_profile = FindAccountProfile::new();
    let find_account_security = FindAccountSecurity::new();
    let (account_profile, account_security) = tokio::try_join!(
        find_account_profile.execute(&app_state.db_pool, current_session.user_id),
        find_account_security.execute(&app_state.db_pool, current_session.user_id),
    )?;
    Ok(Json(AccountResponse::new(
        account_profile,
        account_security,
    )))
}
