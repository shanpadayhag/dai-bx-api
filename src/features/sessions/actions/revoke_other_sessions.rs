use sqlx::PgExecutor;

use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct RevokeOtherSessions {
    postgres_session_source: PostgresSessionSource,
}

impl RevokeOtherSessions {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        current_session: CurrentSession,
        revoked_reason: RevokedReason,
    ) -> Result<u64, SessionError> {
        Ok(self
            .postgres_session_source
            .revoke_all_for_user_except(
                executor,
                current_session.user_id,
                current_session.session_id,
                revoked_reason.as_str(),
            )
            .await?)
    }
}
