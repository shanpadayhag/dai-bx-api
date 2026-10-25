use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::entities::account_profile::{AccountProfile, ProfileChanges};
use crate::features::account::error::AccountError;

#[derive(Default)]
pub struct UpdateAccountProfile {
    postgres_account_source: PostgresAccountSource,
}

impl UpdateAccountProfile {
    pub fn new() -> Self {
        Self {
            postgres_account_source: PostgresAccountSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        profile_changes: &ProfileChanges,
    ) -> Result<AccountProfile, AccountError> {
        self.postgres_account_source
            .update_profile(executor, user_id, profile_changes)
            .await?
            .ok_or(AccountError::NotSignedIn)
    }
}
