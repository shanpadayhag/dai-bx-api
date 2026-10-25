use axum::Json;
use axum::extract::State;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_email_change::ConfirmEmailChange;
use crate::features::account::responses::status_response::StatusResponse;

#[derive(Deserialize)]
pub struct ConfirmEmailChangeRequest {
    pub token: String,
}

pub async fn post_account_email_confirm(
    State(app_state): State<AppState>,
    request_context: RequestContext,
    Json(confirm_request): Json<ConfirmEmailChangeRequest>,
) -> Result<Json<StatusResponse>, AppError> {
    ConfirmEmailChange::new(&app_state)
        .execute(confirm_request.token.trim(), &request_context)
        .await?;
    Ok(Json(StatusResponse::email_changed()))
}
