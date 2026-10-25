use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::email_tokens::entities::email_token::{ConsumedEmailToken, EmailTokenPurpose};

#[derive(Default)]
pub struct PostgresEmailTokenSource;

impl PostgresEmailTokenSource {
    pub async fn retire_unused(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        purpose: EmailTokenPurpose,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE email_tokens SET used_at = now()
             WHERE user_id = $1 AND purpose = $2 AND used_at IS NULL",
            user_id,
            purpose as EmailTokenPurpose,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        purpose: EmailTokenPurpose,
        token_hash: &[u8],
        new_email: Option<&str>,
        valid_for_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO email_tokens (id, user_id, purpose, token_hash, new_email, expires_at)
             VALUES ($1, $2, $3, $4, $5::text::citext, now() + make_interval(secs => $6::float8))",
            Uuid::now_v7(),
            user_id,
            purpose as EmailTokenPurpose,
            token_hash,
            new_email,
            valid_for_seconds as f64,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn consume(
        &self,
        executor: impl PgExecutor<'_>,
        token_hash: &[u8],
        purpose: EmailTokenPurpose,
    ) -> Result<Option<ConsumedEmailToken>, sqlx::Error> {
        sqlx::query_as!(
            ConsumedEmailToken,
            r#"UPDATE email_tokens SET used_at = now()
               WHERE token_hash = $1 AND purpose = $2 AND used_at IS NULL AND expires_at > now()
               RETURNING user_id, new_email::text AS "new_email""#,
            token_hash,
            purpose as EmailTokenPurpose,
        )
        .fetch_optional(executor)
        .await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    pub(crate) async fn seed_user_id(db_pool: &PgPool) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: format!("{}@daibx.test", Uuid::new_v4()),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    fn fresh_token_hash() -> [u8; 32] {
        hash_opaque_token(&generate_opaque_token())
    }

    #[sqlx::test]
    async fn consumes_a_token_once_for_its_own_purpose(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let token_hash = fresh_token_hash();
        PostgresEmailTokenSource
            .insert(
                &db_pool,
                user_id,
                EmailTokenPurpose::VerifyEmail,
                &token_hash,
                None,
                86_400,
            )
            .await
            .expect("insert should succeed");

        let wrong_purpose = PostgresEmailTokenSource
            .consume(&db_pool, &token_hash, EmailTokenPurpose::PasswordReset)
            .await
            .expect("consume should run");
        let first_consumption = PostgresEmailTokenSource
            .consume(&db_pool, &token_hash, EmailTokenPurpose::VerifyEmail)
            .await
            .expect("consume should run");
        let second_consumption = PostgresEmailTokenSource
            .consume(&db_pool, &token_hash, EmailTokenPurpose::VerifyEmail)
            .await
            .expect("consume should run");

        assert_eq!(wrong_purpose, None);
        assert_eq!(
            first_consumption,
            Some(ConsumedEmailToken {
                user_id,
                new_email: None
            })
        );
        assert_eq!(second_consumption, None);
    }

    #[sqlx::test]
    async fn returns_the_new_email_of_a_change_link(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let token_hash = fresh_token_hash();
        PostgresEmailTokenSource
            .insert(
                &db_pool,
                user_id,
                EmailTokenPurpose::ChangeEmail,
                &token_hash,
                Some("new@example.com"),
                86_400,
            )
            .await
            .expect("insert should succeed");

        let consumed_token = PostgresEmailTokenSource
            .consume(&db_pool, &token_hash, EmailTokenPurpose::ChangeEmail)
            .await
            .expect("consume should run")
            .expect("the link should be valid");

        assert_eq!(consumed_token.new_email.as_deref(), Some("new@example.com"));
    }

    #[sqlx::test]
    async fn refuses_an_expired_or_retired_token(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool).await;
        let expired_token_hash = fresh_token_hash();
        let retired_token_hash = fresh_token_hash();
        PostgresEmailTokenSource
            .insert(
                &db_pool,
                user_id,
                EmailTokenPurpose::PasswordReset,
                &expired_token_hash,
                None,
                -60,
            )
            .await
            .expect("insert should succeed");
        PostgresEmailTokenSource
            .insert(
                &db_pool,
                user_id,
                EmailTokenPurpose::PasswordReset,
                &retired_token_hash,
                None,
                1_800,
            )
            .await
            .expect("insert should succeed");

        PostgresEmailTokenSource
            .retire_unused(&db_pool, user_id, EmailTokenPurpose::PasswordReset)
            .await
            .expect("retiring should succeed");

        for refused_token_hash in [expired_token_hash, retired_token_hash] {
            assert_eq!(
                PostgresEmailTokenSource
                    .consume(
                        &db_pool,
                        &refused_token_hash,
                        EmailTokenPurpose::PasswordReset
                    )
                    .await
                    .expect("consume should run"),
                None
            );
        }
    }
}
