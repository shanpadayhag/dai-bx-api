use sqlx::PgPool;
use uuid::Uuid;

use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::entities::push_device::PushDevice;
use crate::features::notifications::error::NotificationError;

pub struct ListPushDevices<'a> {
    db_pool: &'a PgPool,
    postgres_push_device_source: PostgresPushDeviceSource,
}

impl<'a> ListPushDevices<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_push_device_source: PostgresPushDeviceSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<Vec<PushDevice>, NotificationError> {
        Ok(self
            .postgres_push_device_source
            .list_for_user(self.db_pool, user_id)
            .await?)
    }
}
