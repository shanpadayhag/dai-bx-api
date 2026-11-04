use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::entities::time_tracking_settings::{
    TimeTrackingChanges, TimeTrackingSettings, TimeTrackingWrite,
};
use crate::features::account::error::AccountError;

#[derive(Default)]
pub struct UpdateTimeTrackingSettings {
    postgres_account_source: PostgresAccountSource,
}

impl UpdateTimeTrackingSettings {
    pub fn new() -> Self {
        Self {
            postgres_account_source: PostgresAccountSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        time_tracking_changes: &TimeTrackingChanges,
    ) -> Result<TimeTrackingSettings, AccountError> {
        match self
            .postgres_account_source
            .update_time_tracking(executor, user_id, time_tracking_changes)
            .await?
        {
            TimeTrackingWrite::Saved(time_tracking_settings) => Ok(time_tracking_settings),
            TimeTrackingWrite::CategoryMissing => Err(AccountError::CategoryNotFound),
            TimeTrackingWrite::AccountMissing => Err(AccountError::NotSignedIn),
        }
    }
}
