use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::sessions::actions::issue_session_tokens::IssueSessionTokens;
use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::session::{
    IssuedSessionTokens, REFRESH_GRACE_SECONDS, RevokedReason, SESSION_ABSOLUTE_LIMIT_SECONDS,
    SESSION_IDLE_LIMIT_SECONDS,
};
use crate::features::sessions::error::SessionError;

pub enum SessionRenewal {
    Renewed { session_tokens: IssuedSessionTokens },
    ReuseDetected { user_id: Uuid },
    Refused,
}

#[derive(Default)]
pub struct RenewSession {
    postgres_session_source: PostgresSessionSource,
    issue_session_tokens: IssueSessionTokens,
}

impl RenewSession {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
            issue_session_tokens: IssueSessionTokens::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        refresh_token: &str,
    ) -> Result<SessionRenewal, SessionError> {
        let Some(presented_token) = self
            .postgres_session_source
            .find_refresh_token_for_update(
                &mut *connection,
                &hash_opaque_token(refresh_token),
                SESSION_IDLE_LIMIT_SECONDS,
                SESSION_ABSOLUTE_LIMIT_SECONDS,
                REFRESH_GRACE_SECONDS,
            )
            .await?
        else {
            return Ok(SessionRenewal::Refused);
        };
        if !presented_token.is_session_live {
            return Ok(SessionRenewal::Refused);
        }
        if !presented_token.is_replaced {
            self.postgres_session_source
                .mark_token_replaced(&mut *connection, presented_token.token_id)
                .await?;
        } else if presented_token.is_within_grace {
            self.postgres_session_source
                .mark_grace_used(&mut *connection, presented_token.token_id)
                .await?;
        } else {
            self.postgres_session_source
                .revoke(
                    &mut *connection,
                    presented_token.session_id,
                    RevokedReason::RefreshTokenReuse.as_str(),
                )
                .await?;
            return Ok(SessionRenewal::ReuseDetected {
                user_id: presented_token.user_id,
            });
        }
        self.postgres_session_source
            .mark_seen(&mut *connection, presented_token.session_id)
            .await?;
        let session_tokens = self
            .issue_session_tokens
            .execute(connection, presented_token.session_id)
            .await?;
        Ok(SessionRenewal::Renewed { session_tokens })
    }
}
