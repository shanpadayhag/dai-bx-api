use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Default)]
pub struct PostgresKnownBrowserSource;

impl PostgresKnownBrowserSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        token_hash: &[u8],
        valid_for_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO known_browsers (id, user_id, token_hash, expires_at)
             VALUES ($1, $2, $3, now() + make_interval(secs => $4::float8))",
            Uuid::now_v7(),
            user_id,
            token_hash,
            valid_for_seconds as f64,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn extend_active(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        token_hash: &[u8],
        valid_for_seconds: i64,
    ) -> Result<bool, sqlx::Error> {
        let extended_row_count = sqlx::query!(
            "UPDATE known_browsers
             SET last_seen_at = now(), expires_at = now() + make_interval(secs => $3::float8)
             WHERE user_id = $1 AND token_hash = $2 AND expires_at > now()",
            user_id,
            token_hash,
            valid_for_seconds as f64,
        )
        .execute(executor)
        .await?
        .rows_affected();
        Ok(extended_row_count == 1)
    }

    pub async fn delete_all_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM known_browsers WHERE user_id = $1", user_id)
            .execute(executor)
            .await?;
        Ok(())
    }
}
