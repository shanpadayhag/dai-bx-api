use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::remembered_devices::entities::remembered_device::RememberedDevice;

#[derive(Default)]
pub struct PostgresRememberedDeviceSource;

impl PostgresRememberedDeviceSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        token_hash: &[u8],
        user_agent: Option<&str>,
        valid_for_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO remembered_devices (id, user_id, token_hash, user_agent, expires_at)
             VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5::float8))",
            Uuid::now_v7(),
            user_id,
            token_hash,
            user_agent,
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
    ) -> Result<Option<RememberedDevice>, sqlx::Error> {
        sqlx::query_as!(
            RememberedDevice,
            "SELECT id, user_id
             FROM remembered_devices
             WHERE token_hash = $1 AND expires_at > now() AND revoked_at IS NULL",
            token_hash,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn mark_used(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE remembered_devices SET last_used_at = now() WHERE id = $1",
            device_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn revoke_all_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query!(
            "UPDATE remembered_devices SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
            user_id,
        )
        .execute(executor)
        .await?
        .rows_affected())
    }
}
