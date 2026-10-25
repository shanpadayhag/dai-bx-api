use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::entities::account_profile::{
    AccentColor, AccountProfile, ProfileChanges, ThemeMode,
};
use crate::features::account::entities::account_security::AccountSecurity;

#[derive(Default)]
pub struct PostgresAccountSource;

impl PostgresAccountSource {
    pub async fn find_profile(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<AccountProfile>, sqlx::Error> {
        sqlx::query_as!(
            AccountProfile,
            r#"SELECT name,
                      email::text AS "email!",
                      accent_color AS "accent_color: AccentColor",
                      theme AS "theme: ThemeMode"
               FROM users
               WHERE id = $1"#,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_security(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        idle_limit_seconds: i64,
        absolute_limit_seconds: i64,
    ) -> Result<AccountSecurity, sqlx::Error> {
        sqlx::query_as!(
            AccountSecurity,
            r#"SELECT
                 (SELECT count(*)
                  FROM recovery_codes
                  WHERE user_id = $1 AND used_at IS NULL) AS "recovery_codes_remaining!",
                 (SELECT count(*)
                  FROM sessions
                  WHERE user_id = $1
                    AND revoked_at IS NULL
                    AND last_seen_at > now() - make_interval(secs => $2::float8)
                    AND created_at > now() - make_interval(secs => $3::float8)) AS "active_session_count!""#,
            user_id,
            idle_limit_seconds as f64,
            absolute_limit_seconds as f64,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn update_profile(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        profile_changes: &ProfileChanges,
    ) -> Result<Option<AccountProfile>, sqlx::Error> {
        sqlx::query_as!(
            AccountProfile,
            r#"UPDATE users
               SET name = coalesce($2, name),
                   accent_color = coalesce($3, accent_color),
                   theme = coalesce($4, theme),
                   updated_at = now()
               WHERE id = $1
               RETURNING name,
                         email::text AS "email!",
                         accent_color AS "accent_color: AccentColor",
                         theme AS "theme: ThemeMode""#,
            user_id,
            profile_changes.name.as_deref(),
            profile_changes.accent_color as Option<AccentColor>,
            profile_changes.theme as Option<ThemeMode>,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn change_email(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        new_email: &str,
    ) -> Result<bool, sqlx::Error> {
        let change_result = sqlx::query!(
            "UPDATE users
             SET email = $2::text::citext,
                 email_verified_at = coalesce(email_verified_at, now()),
                 updated_at = now()
             WHERE id = $1",
            user_id,
            new_email,
        )
        .execute(executor)
        .await;
        match change_result {
            Ok(change_outcome) => Ok(change_outcome.rows_affected() == 1),
            Err(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
                Ok(false)
            }
            Err(other_error) => Err(other_error),
        }
    }
}
