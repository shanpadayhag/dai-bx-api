use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::session::{
    IssuedSessionTokens, SESSION_ABSOLUTE_LIMIT_SECONDS, SessionTokenKind,
};
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct IssueSessionTokens {
    postgres_session_source: PostgresSessionSource,
}

impl IssueSessionTokens {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        session_id: Uuid,
    ) -> Result<IssuedSessionTokens, SessionError> {
        let access_token = generate_opaque_token();
        let access_valid_for_seconds = self
            .postgres_session_source
            .insert_token(
                &mut *connection,
                session_id,
                SessionTokenKind::Access,
                &hash_opaque_token(&access_token),
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await?;
        let refresh_token = generate_opaque_token();
        let refresh_valid_for_seconds = self
            .postgres_session_source
            .insert_token(
                &mut *connection,
                session_id,
                SessionTokenKind::Refresh,
                &hash_opaque_token(&refresh_token),
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await?;
        Ok(IssuedSessionTokens {
            access_token,
            access_valid_for_seconds,
            refresh_token,
            refresh_valid_for_seconds,
        })
    }
}
