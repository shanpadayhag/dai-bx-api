use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::account::responses::account_sessions_response::AccountSessionsResponse;
use crate::features::sessions::actions::list_live_sessions::ListLiveSessions;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_account_sessions(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<AccountSessionsResponse>, AppError> {
    let session_summaries = ListLiveSessions::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(AccountSessionsResponse::new(
        session_summaries,
        current_session.session_id,
    )))
}
