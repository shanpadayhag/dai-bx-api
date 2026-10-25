use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::auth_challenges::entities::auth_challenge::{
    AuthChallenge, AuthChallengePurpose,
};

#[derive(Default)]
pub struct PostgresAuthChallengeSource;

impl PostgresAuthChallengeSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        purpose: AuthChallengePurpose,
        token_hash: &[u8],
        pending_totp_secret: Option<&[u8]>,
        valid_for_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO auth_challenges (id, user_id, purpose, token_hash, pending_totp_secret, expires_at)
             VALUES ($1, $2, $3, $4, $5, now() + make_interval(secs => $6::float8))",
            Uuid::now_v7(),
            user_id,
            purpose as AuthChallengePurpose,
            token_hash,
            pending_totp_secret,
            valid_for_seconds as f64,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn find_active(
        &self,
        executor: impl PgExecutor<'_>,
        token_hash: &[u8],
    ) -> Result<Option<AuthChallenge>, sqlx::Error> {
        sqlx::query_as!(
            AuthChallenge,
            r#"SELECT id,
                      user_id,
                      purpose AS "purpose: AuthChallengePurpose",
                      pending_totp_secret,
                      attempts
               FROM auth_challenges
               WHERE token_hash = $1 AND expires_at > now()"#,
            token_hash,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn claim_attempt(
        &self,
        executor: impl PgExecutor<'_>,
        token_hash: &[u8],
        purpose: AuthChallengePurpose,
        max_attempts: i16,
    ) -> Result<Option<AuthChallenge>, sqlx::Error> {
        sqlx::query_as!(
            AuthChallenge,
            r#"UPDATE auth_challenges
               SET attempts = attempts + 1
               WHERE token_hash = $1 AND purpose = $2 AND expires_at > now() AND attempts < $3
               RETURNING id,
                         user_id,
                         purpose AS "purpose: AuthChallengePurpose",
                         pending_totp_secret,
                         attempts"#,
            token_hash,
            purpose as AuthChallengePurpose,
            max_attempts,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        challenge_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM auth_challenges WHERE id = $1", challenge_id)
            .execute(executor)
            .await?;
        Ok(())
    }
}
