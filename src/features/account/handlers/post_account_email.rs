use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::request_email_change::{
    EmailChangeRequest, RequestEmailChange,
};
use crate::features::account::responses::status_response::StatusResponse;
use crate::features::auth::handlers::post_sign_up::validate_email;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEmailRequest {
    pub password: String,
    pub code: String,
    pub new_email: String,
}

pub async fn post_account_email(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_context: RequestContext,
    Json(change_request): Json<ChangeEmailRequest>,
) -> Result<(StatusCode, Json<StatusResponse>), AppError> {
    let new_email = validate_email(&change_request.new_email)?;
    RequestEmailChange::new(&app_state)
        .execute(
            current_session,
            EmailChangeRequest {
                password: Zeroizing::new(change_request.password),
                entered_code: change_request.code.trim(),
                new_email,
            },
            &request_context,
        )
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(StatusResponse::confirmation_email_sent()),
    ))
}
