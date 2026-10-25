use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::recovery_codes::entities::stored_recovery_code::StoredRecoveryCode;

#[derive(Default)]
pub struct PostgresRecoveryCodeSource;

impl PostgresRecoveryCodeSource {
    pub async fn replace_all(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        code_phcs: &[String],
    ) -> Result<(), sqlx::Error> {
        let code_ids: Vec<Uuid> = code_phcs.iter().map(|_| Uuid::now_v7()).collect();
        sqlx::query!(
            "WITH retired_codes AS (DELETE FROM recovery_codes WHERE user_id = $1)
             INSERT INTO recovery_codes (id, user_id, code_phc)
             SELECT new_code.id, $1, new_code.code_phc
             FROM UNNEST($2::uuid[], $3::text[]) AS new_code (id, code_phc)",
            user_id,
            &code_ids,
            code_phcs,
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
            r#"SELECT id,
                      code_phc,
                      salt AS legacy_salt,
                      code_hash AS legacy_code_hash
               FROM recovery_codes
               WHERE user_id = $1 AND used_at IS NULL
               ORDER BY id
               FOR UPDATE"#,
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
