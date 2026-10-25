use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::entities::account_security::AccountSecurity;
use crate::features::account::error::AccountError;
use crate::features::sessions::entities::session::{
    SESSION_ABSOLUTE_LIMIT_SECONDS, SESSION_IDLE_LIMIT_SECONDS,
};

#[derive(Default)]
pub struct FindAccountSecurity {
    postgres_account_source: PostgresAccountSource,
}

impl FindAccountSecurity {
    pub fn new() -> Self {
        Self {
            postgres_account_source: PostgresAccountSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<AccountSecurity, AccountError> {
        Ok(self
            .postgres_account_source
            .find_security(
                executor,
                user_id,
                SESSION_IDLE_LIMIT_SECONDS,
                SESSION_ABSOLUTE_LIMIT_SECONDS,
            )
            .await?)
    }
}
