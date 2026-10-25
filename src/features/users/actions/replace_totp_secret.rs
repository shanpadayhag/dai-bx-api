use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct ReplaceTotpSecret {
    postgres_user_source: PostgresUserSource,
}

impl ReplaceTotpSecret {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        sealed_totp_secret: &[u8],
        accepted_totp_step: u64,
    ) -> Result<(), UserError> {
        let accepted_totp_step =
            i64::try_from(accepted_totp_step).expect("TOTP steps stay far below i64::MAX");
        Ok(self
            .postgres_user_source
            .replace_totp_secret(executor, user_id, sealed_totp_secret, accepted_totp_step)
            .await?)
    }
}
