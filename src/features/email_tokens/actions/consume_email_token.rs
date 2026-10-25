use sqlx::PgExecutor;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::email_tokens::data_sources::postgres::email_token_source::PostgresEmailTokenSource;
use crate::features::email_tokens::entities::email_token::{ConsumedEmailToken, EmailTokenPurpose};
use crate::features::email_tokens::error::EmailTokenError;

const MAX_LINK_TOKEN_LENGTH: usize = 128;

#[derive(Default)]
pub struct ConsumeEmailToken {
    postgres_email_token_source: PostgresEmailTokenSource,
}

impl ConsumeEmailToken {
    pub fn new() -> Self {
        Self {
            postgres_email_token_source: PostgresEmailTokenSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        link_token: &str,
        purpose: EmailTokenPurpose,
    ) -> Result<Option<ConsumedEmailToken>, EmailTokenError> {
        if link_token.is_empty() || link_token.len() > MAX_LINK_TOKEN_LENGTH {
            return Ok(None);
        }
        Ok(self
            .postgres_email_token_source
            .consume(executor, &hash_opaque_token(link_token), purpose)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::email_tokens::actions::issue_email_token::IssueEmailToken;
    use crate::features::email_tokens::data_sources::postgres::email_token_source::tests::seed_user_id;

    async fn issue(db_pool: &PgPool, user_id: uuid::Uuid) -> String {
        let mut connection = db_pool.acquire().await.expect("connection should open");
        IssueEmailToken::new()
            .execute(
                &mut connection,
                user_id,
                EmailTokenPurpose::VerifyEmail,
                None,
            )
            .await
            .expect("issuing should succeed")
            .to_string()
    }

    #[sqlx::test]
    async fn consumes_an_issued_token_once(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let link_token = issue(&db_pool, user_id).await;

        let first_consumption = ConsumeEmailToken::new()
            .execute(&db_pool, &link_token, EmailTokenPurpose::VerifyEmail)
            .await
            .expect("consuming should run");
        let second_consumption = ConsumeEmailToken::new()
            .execute(&db_pool, &link_token, EmailTokenPurpose::VerifyEmail)
            .await
            .expect("consuming should run");

        assert_eq!(link_token.len(), 43);
        assert_eq!(
            first_consumption.map(|consumed_token| consumed_token.user_id),
            Some(user_id)
        );
        assert_eq!(second_consumption, None);
    }

    #[sqlx::test]
    async fn retires_the_earlier_link_when_a_new_one_is_issued(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let earlier_link_token = issue(&db_pool, user_id).await;
        let later_link_token = issue(&db_pool, user_id).await;

        let earlier_consumption = ConsumeEmailToken::new()
            .execute(
                &db_pool,
                &earlier_link_token,
                EmailTokenPurpose::VerifyEmail,
            )
            .await
            .expect("consuming should run");
        let later_consumption = ConsumeEmailToken::new()
            .execute(&db_pool, &later_link_token, EmailTokenPurpose::VerifyEmail)
            .await
            .expect("consuming should run");

        assert_eq!(earlier_consumption, None);
        assert!(later_consumption.is_some());
    }

    #[sqlx::test]
    async fn ignores_an_empty_or_oversized_token(db_pool: PgPool) {
        for refused_token in [String::new(), "x".repeat(500)] {
            assert_eq!(
                ConsumeEmailToken::new()
                    .execute(&db_pool, &refused_token, EmailTokenPurpose::VerifyEmail)
                    .await
                    .expect("consuming should run"),
                None
            );
        }
    }
}
