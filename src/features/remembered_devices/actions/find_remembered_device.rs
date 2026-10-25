use sqlx::PgExecutor;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::remembered_devices::data_sources::postgres::remembered_device_source::PostgresRememberedDeviceSource;
use crate::features::remembered_devices::entities::remembered_device::RememberedDevice;
use crate::features::remembered_devices::error::RememberedDeviceError;

#[derive(Default)]
pub struct FindRememberedDevice {
    postgres_remembered_device_source: PostgresRememberedDeviceSource,
}

impl FindRememberedDevice {
    pub fn new() -> Self {
        Self {
            postgres_remembered_device_source: PostgresRememberedDeviceSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        device_token: &str,
    ) -> Result<Option<RememberedDevice>, RememberedDeviceError> {
        Ok(self
            .postgres_remembered_device_source
            .find_active(executor, &hash_opaque_token(device_token))
            .await?)
    }
}
