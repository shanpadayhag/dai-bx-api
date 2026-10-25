use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::IdentityProof;
use crate::features::account::actions::regenerate_recovery_codes::RegenerateRecoveryCodes;
use crate::features::auth::responses::recovery_codes_response::RecoveryCodesResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct RegenerateRecoveryCodesRequest {
    pub password: String,
    pub code: String,
}

pub async fn post_account_recovery_codes(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(regenerate_request): Json<RegenerateRecoveryCodesRequest>,
) -> Result<Response, AppError> {
    let recovery_codes = RegenerateRecoveryCodes::new(&app_state)
        .execute(
            current_session,
            IdentityProof {
                password: Zeroizing::new(regenerate_request.password),
                entered_code: regenerate_request.code.trim(),
            },
            &request_context,
        )
        .await?;
    Ok(Json(RecoveryCodesResponse::from_codes(&recovery_codes)).into_response())
}
