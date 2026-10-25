use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::entities::account_profile::AccountProfile;
use crate::features::account::error::AccountError;

#[derive(Default)]
pub struct FindAccountProfile {
    postgres_account_source: PostgresAccountSource,
}

impl FindAccountProfile {
    pub fn new() -> Self {
        Self {
            postgres_account_source: PostgresAccountSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<AccountProfile, AccountError> {
        self.postgres_account_source
            .find_profile(executor, user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)
    }
}
