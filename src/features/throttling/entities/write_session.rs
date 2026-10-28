use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::throttling::actions::allow_write::AllowWrite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for WriteSession {
    type Rejection = AppError;

    async fn from_request_parts(
        request_parts: &mut Parts,
        app_state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current_session = CurrentSession::from_request_parts(request_parts, app_state).await?;
        AllowWrite::new(&app_state.db_pool)
            .execute(current_session.user_id)
            .await?;
        Ok(Self {
            session_id: current_session.session_id,
            user_id: current_session.user_id,
        })
    }
}
