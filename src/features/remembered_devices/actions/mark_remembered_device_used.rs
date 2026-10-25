use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::remembered_devices::data_sources::postgres::remembered_device_source::PostgresRememberedDeviceSource;
use crate::features::remembered_devices::error::RememberedDeviceError;

#[derive(Default)]
pub struct MarkRememberedDeviceUsed {
    postgres_remembered_device_source: PostgresRememberedDeviceSource,
}

impl MarkRememberedDeviceUsed {
    pub fn new() -> Self {
        Self {
            postgres_remembered_device_source: PostgresRememberedDeviceSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
    ) -> Result<(), RememberedDeviceError> {
        Ok(self
            .postgres_remembered_device_source
            .mark_used(executor, device_id)
            .await?)
    }
}
