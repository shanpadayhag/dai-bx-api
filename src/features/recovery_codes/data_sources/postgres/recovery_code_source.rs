use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::security::recovery_code::RecoveryCodeHash;
use crate::features::recovery_codes::entities::stored_recovery_code::StoredRecoveryCode;

#[derive(Default)]
pub struct PostgresRecoveryCodeSource;

impl PostgresRecoveryCodeSource {
    pub async fn replace_all(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        recovery_code_hashes: &[RecoveryCodeHash],
    ) -> Result<(), sqlx::Error> {
        let code_ids: Vec<Uuid> = recovery_code_hashes
            .iter()
            .map(|_| Uuid::now_v7())
            .collect();
        let code_salts: Vec<Vec<u8>> = recovery_code_hashes
            .iter()
            .map(|recovery_code_hash| recovery_code_hash.salt.to_vec())
            .collect();
        let code_hashes: Vec<Vec<u8>> = recovery_code_hashes
            .iter()
            .map(|recovery_code_hash| recovery_code_hash.code_hash.to_vec())
            .collect();
        sqlx::query!(
            "WITH retired_codes AS (DELETE FROM recovery_codes WHERE user_id = $1)
             INSERT INTO recovery_codes (id, user_id, salt, code_hash)
             SELECT new_code.id, $1, new_code.salt, new_code.code_hash
             FROM UNNEST($2::uuid[], $3::bytea[], $4::bytea[]) AS new_code (id, salt, code_hash)",
            user_id,
            &code_ids,
            &code_salts,
            &code_hashes,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn count_unused(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "unused_count!" FROM recovery_codes WHERE user_id = $1 AND used_at IS NULL"#,
            user_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn list_unused_for_update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<StoredRecoveryCode>, sqlx::Error> {
        sqlx::query_as!(
            StoredRecoveryCode,
            "SELECT id, salt, code_hash
             FROM recovery_codes
             WHERE user_id = $1 AND used_at IS NULL
             ORDER BY id
             FOR UPDATE",
            user_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn mark_used(
        &self,
        executor: impl PgExecutor<'_>,
        recovery_code_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE recovery_codes SET used_at = now() WHERE id = $1",
            recovery_code_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}
