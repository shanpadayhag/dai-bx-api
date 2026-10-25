use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct EndOwnedSession {
    postgres_session_source: PostgresSessionSource,
}

impl EndOwnedSession {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        session_id: Uuid,
        user_id: Uuid,
        revoked_reason: RevokedReason,
    ) -> Result<bool, SessionError> {
        Ok(self
            .postgres_session_source
            .revoke_owned(executor, session_id, user_id, revoked_reason.as_str())
            .await?)
    }
}
