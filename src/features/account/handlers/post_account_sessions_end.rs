use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::end_other_session::EndOtherSession;
use crate::features::account::error::AccountError;
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndSessionRequest {
    pub session_id: String,
    pub password: String,
}

pub async fn post_account_sessions_end(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(end_request): Json<EndSessionRequest>,
) -> Result<Json<StatusResponse>, AppError> {
    let ended_session_id = Uuid::parse_str(end_request.session_id.trim())
        .map_err(|_| AccountError::SessionAlreadyEnded)?;
    EndOtherSession::new(&app_state)
        .execute(
            current_session,
            ended_session_id,
            Zeroizing::new(end_request.password),
            &request_context,
        )
        .await?;
    Ok(Json(StatusResponse::session_ended()))
}
