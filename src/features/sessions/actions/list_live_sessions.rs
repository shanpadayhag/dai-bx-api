use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::session::{
    SESSION_ABSOLUTE_LIMIT_SECONDS, SESSION_IDLE_LIMIT_SECONDS,
};
use crate::features::sessions::entities::session_summary::SessionSummary;
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct ListLiveSessions {
    postgres_session_source: PostgresSessionSource,
}

impl ListLiveSessions {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<SessionSummary>, SessionError> {
        Ok(self
            .postgres_session_source
            .list_live_for_user(
                executor,
                user_id,
                SESSION_IDLE_LIMIT_SECONDS,
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await?)
    }
}
