use sqlx::PgExecutor;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::session::{
    ActiveSession, SESSION_ABSOLUTE_LIMIT_SECONDS, SESSION_IDLE_LIMIT_SECONDS,
};
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct FindActiveSession {
    postgres_session_source: PostgresSessionSource,
}

impl FindActiveSession {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        access_token: &str,
    ) -> Result<Option<ActiveSession>, SessionError> {
        Ok(self
            .postgres_session_source
            .find_active_by_access_token_hash(
                executor,
                &hash_opaque_token(access_token),
                SESSION_IDLE_LIMIT_SECONDS,
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await?)
    }
}
