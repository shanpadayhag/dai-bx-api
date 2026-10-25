use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::core::cookie::read_cookie;
use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::sessions::actions::find_active_session::FindActiveSession;
use crate::features::sessions::entities::session::ACCESS_COOKIE_NAME;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for CurrentSession {
    type Rejection = AppError;

    async fn from_request_parts(
        request_parts: &mut Parts,
        app_state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let access_token = read_cookie(&request_parts.headers, ACCESS_COOKIE_NAME)
            .ok_or(AppError::Unauthorized)?;
        let active_session = FindActiveSession::new()
            .execute(&app_state.db_pool, access_token)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(Self {
            session_id: active_session.session_id,
            user_id: active_session.user_id,
        })
    }
}
