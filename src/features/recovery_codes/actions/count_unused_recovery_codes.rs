use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::recovery_codes::data_sources::postgres::recovery_code_source::PostgresRecoveryCodeSource;
use crate::features::recovery_codes::error::RecoveryCodeError;

#[derive(Default)]
pub struct CountUnusedRecoveryCodes {
    postgres_recovery_code_source: PostgresRecoveryCodeSource,
}

impl CountUnusedRecoveryCodes {
    pub fn new() -> Self {
        Self {
            postgres_recovery_code_source: PostgresRecoveryCodeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<i64, RecoveryCodeError> {
        Ok(self
            .postgres_recovery_code_source
            .count_unused(executor, user_id)
            .await?)
    }
}
