use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::entities::user::{NewUser, TotpState, User};

#[derive(Default)]
pub struct PostgresUserSource;

impl PostgresUserSource {
    pub async fn insert_if_email_free(
        &self,
        executor: impl PgExecutor<'_>,
        new_user: &NewUser,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
               VALUES ($1, $2, $3, $4::text::citext, $5, $6, now())
               ON CONFLICT (email) DO NOTHING
               RETURNING id,
                      security_ref,
                      name,
                      email::text AS "email!",
                      password_hash,
                      email_verified_at IS NOT NULL AS "is_email_verified!",
                      mfa_enabled_at IS NOT NULL AS "is_mfa_enabled!",
                      terms_version"#,
            Uuid::now_v7(),
            Uuid::new_v4(),
            new_user.name,
            new_user.email,
            new_user.password_hash,
            new_user.terms_version,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_by_email(
        &self,
        executor: impl PgExecutor<'_>,
        email: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"SELECT id,
                      security_ref,
                      name,
                      email::text AS "email!",
                      password_hash,
                      email_verified_at IS NOT NULL AS "is_email_verified!",
                      mfa_enabled_at IS NOT NULL AS "is_mfa_enabled!",
                      terms_version
               FROM users
               WHERE email = $1::text::citext"#,
            email,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_by_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"SELECT id,
                      security_ref,
                      name,
                      email::text AS "email!",
                      password_hash,
                      email_verified_at IS NOT NULL AS "is_email_verified!",
                      mfa_enabled_at IS NOT NULL AS "is_mfa_enabled!",
                      terms_version
               FROM users
               WHERE id = $1"#,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn mark_email_verified(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE users
             SET email_verified_at = coalesce(email_verified_at, now()), updated_at = now()
             WHERE id = $1",
            user_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn enable_mfa(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        sealed_totp_secret: &[u8],
        accepted_totp_step: i64,
    ) -> Result<bool, sqlx::Error> {
        let updated_rows = sqlx::query!(
            "UPDATE users
             SET totp_secret_encrypted = $2,
                 totp_last_used_step = $3,
                 mfa_enabled_at = now(),
                 updated_at = now()
             WHERE id = $1 AND mfa_enabled_at IS NULL",
            user_id,
            sealed_totp_secret,
            accepted_totp_step,
        )
        .execute(executor)
        .await?
        .rows_affected();
        Ok(updated_rows == 1)
    }

    pub async fn find_totp_state_for_update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<TotpState>, sqlx::Error> {
        sqlx::query_as!(
            TotpState,
            "SELECT totp_secret_encrypted AS sealed_totp_secret, totp_last_used_step AS last_used_step
             FROM users
             WHERE id = $1
             FOR UPDATE",
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn advance_totp_step(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        accepted_totp_step: i64,
    ) -> Result<bool, sqlx::Error> {
        let updated_rows = sqlx::query!(
            "UPDATE users
             SET totp_last_used_step = $2, updated_at = now()
             WHERE id = $1 AND (totp_last_used_step IS NULL OR totp_last_used_step < $2)",
            user_id,
            accepted_totp_step,
        )
        .execute(executor)
        .await?
        .rows_affected();
        Ok(updated_rows == 1)
    }

    pub async fn replace_totp_secret(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        sealed_totp_secret: &[u8],
        accepted_totp_step: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE users
             SET totp_secret_encrypted = $2, totp_last_used_step = $3, updated_at = now()
             WHERE id = $1",
            user_id,
            sealed_totp_secret,
            accepted_totp_step,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM users WHERE id = $1", user_id)
            .execute(executor)
            .await?;
        Ok(())
    }

    pub async fn update_password_hash(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1",
            user_id,
            password_hash,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use sqlx::PgPool;

    use super::*;

    pub(crate) fn new_user_with_email(email: &str) -> NewUser {
        NewUser {
            name: String::from("Shan Padayhag"),
            email: email.to_owned(),
            password_hash: String::from("$argon2id$v=19$m=19456,t=2,p=1$placeholder"),
            terms_version: String::from("test-terms-1"),
        }
    }

    #[sqlx::test]
    async fn creates_an_unverified_user_without_mfa(db_pool: PgPool) {
        let created_user = PostgresUserSource
            .insert_if_email_free(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("insert should succeed")
            .expect("the email should be free");

        assert_eq!(created_user.id.get_version_num(), 7);
        assert_ne!(created_user.security_ref, created_user.id);
        assert!(!created_user.is_email_verified);
        assert!(!created_user.is_mfa_enabled);
        assert_eq!(created_user.terms_version, "test-terms-1");
    }

    #[sqlx::test]
    async fn refuses_an_email_that_is_taken_in_any_case(db_pool: PgPool) {
        PostgresUserSource
            .insert_if_email_free(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("insert should succeed");

        let repeated_insert = PostgresUserSource
            .insert_if_email_free(&db_pool, &new_user_with_email("PERSON@Example.com"))
            .await
            .expect("insert should run");

        assert_eq!(repeated_insert, None);
    }

    #[sqlx::test]
    async fn finds_a_user_by_email_in_any_case_and_marks_the_email_verified(db_pool: PgPool) {
        let created_user = PostgresUserSource
            .insert_if_email_free(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("insert should succeed")
            .expect("the email should be free");

        PostgresUserSource
            .mark_email_verified(&db_pool, created_user.id)
            .await
            .expect("marking should succeed");
        let found_user = PostgresUserSource
            .find_by_email(&db_pool, "Person@EXAMPLE.com")
            .await
            .expect("lookup should succeed")
            .expect("the user should be found");

        assert_eq!(found_user.id, created_user.id);
        assert!(found_user.is_email_verified);
        assert_eq!(
            PostgresUserSource
                .find_by_email(&db_pool, "nobody@example.com")
                .await
                .expect("lookup should succeed"),
            None
        );
    }

    #[sqlx::test]
    async fn enables_mfa_only_once(db_pool: PgPool) {
        let created_user = PostgresUserSource
            .insert_if_email_free(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("insert should succeed")
            .expect("the email should be free");

        let first_enable = PostgresUserSource
            .enable_mfa(&db_pool, created_user.id, b"sealed-secret", 58_000_000)
            .await
            .expect("enabling should run");
        let second_enable = PostgresUserSource
            .enable_mfa(&db_pool, created_user.id, b"other-secret", 58_000_001)
            .await
            .expect("enabling should run");
        let enabled_user = PostgresUserSource
            .find_by_id(&db_pool, created_user.id)
            .await
            .expect("lookup should succeed")
            .expect("the user should exist");

        assert!(first_enable);
        assert!(!second_enable);
        assert!(enabled_user.is_mfa_enabled);
    }
}
