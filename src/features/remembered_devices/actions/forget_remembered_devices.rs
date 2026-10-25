use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::remembered_devices::data_sources::postgres::remembered_device_source::PostgresRememberedDeviceSource;
use crate::features::remembered_devices::error::RememberedDeviceError;

#[derive(Default)]
pub struct ForgetRememberedDevices {
    postgres_remembered_device_source: PostgresRememberedDeviceSource,
}

impl ForgetRememberedDevices {
    pub fn new() -> Self {
        Self {
            postgres_remembered_device_source: PostgresRememberedDeviceSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<u64, RememberedDeviceError> {
        Ok(self
            .postgres_remembered_device_source
            .revoke_all_for_user(executor, user_id)
            .await?)
    }
}
