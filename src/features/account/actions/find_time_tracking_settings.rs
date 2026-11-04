use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::entities::time_tracking_settings::TimeTrackingSettings;
use crate::features::account::error::AccountError;

#[derive(Default)]
pub struct FindTimeTrackingSettings {
    postgres_account_source: PostgresAccountSource,
}

impl FindTimeTrackingSettings {
    pub fn new() -> Self {
        Self {
            postgres_account_source: PostgresAccountSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<TimeTrackingSettings, AccountError> {
        self.postgres_account_source
            .find_time_tracking(executor, user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)
    }
}
