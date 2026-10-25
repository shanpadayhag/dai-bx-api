use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::request_context::RequestContext;
use crate::features::sessions::actions::issue_session_tokens::IssueSessionTokens;
use crate::features::sessions::data_sources::postgres::session_source::PostgresSessionSource;
use crate::features::sessions::entities::presented_session::PresentedSession;
use crate::features::sessions::entities::session::{IssuedSessionTokens, RevokedReason};
use crate::features::sessions::error::SessionError;

#[derive(Default)]
pub struct CreateSession {
    postgres_session_source: PostgresSessionSource,
    issue_session_tokens: IssueSessionTokens,
}

impl CreateSession {
    pub fn new() -> Self {
        Self {
            postgres_session_source: PostgresSessionSource,
            issue_session_tokens: IssueSessionTokens::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        request_context: &RequestContext,
        presented_session: &PresentedSession,
    ) -> Result<IssuedSessionTokens, SessionError> {
        let presented_token_hashes: Vec<Vec<u8>> = presented_session
            .token_hashes()
            .iter()
            .map(|token_hash| token_hash.to_vec())
            .collect();
        if !presented_token_hashes.is_empty() {
            self.postgres_session_source
                .revoke_by_token_hashes(
                    &mut *connection,
                    &presented_token_hashes,
                    RevokedReason::ReplacedBySignIn.as_str(),
                )
                .await?;
        }
        let session_id = self
            .postgres_session_source
            .insert_session(
                &mut *connection,
                user_id,
                request_context.client_ip,
                request_context.user_agent.as_deref(),
            )
            .await?;
        self.issue_session_tokens
            .execute(connection, session_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::sessions::actions::find_active_session::FindActiveSession;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_user_id(db_pool: &PgPool) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: String::from("person@example.com"),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    #[sqlx::test]
    async fn issues_a_working_access_token_and_a_separate_refresh_token(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        let issued_tokens = CreateSession::new()
            .execute(
                &mut connection,
                user_id,
                &RequestContext::default(),
                &PresentedSession::default(),
            )
            .await
            .expect("creating the session should succeed");

        assert_ne!(*issued_tokens.access_token, *issued_tokens.refresh_token);
        let active_session = FindActiveSession::new()
            .execute(&db_pool, &issued_tokens.access_token)
            .await
            .expect("lookup should succeed")
            .expect("the session should be active");
        assert_eq!(active_session.user_id, user_id);
    }
}
