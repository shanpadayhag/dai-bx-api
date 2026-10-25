use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::account::actions::find_account_profile::FindAccountProfile;
use crate::features::auth::responses::session_response::SessionResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_session(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<SessionResponse>, AppError> {
    let account_profile = FindAccountProfile::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(account_profile.into()))
}
