use sqlx::PgPool;
use uuid::Uuid;

use crate::features::notifications::actions::record_push_device_event::{
    PushDeviceChange, RecordPushDeviceEvent,
};
use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::entities::push_device::PushDeviceRemovalReason;
use crate::features::notifications::error::NotificationError;

pub struct RemoveGonePushDevice<'a> {
    db_pool: &'a PgPool,
    postgres_push_device_source: PostgresPushDeviceSource,
    record_push_device_event: RecordPushDeviceEvent,
}

impl<'a> RemoveGonePushDevice<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_push_device_source: PostgresPushDeviceSource,
            record_push_device_event: RecordPushDeviceEvent::new(),
        }
    }

    pub async fn execute(&self, device_id: Uuid) -> Result<(), NotificationError> {
        let mut remove_transaction = self.db_pool.begin().await?;
        if let Some(removed_device) = self
            .postgres_push_device_source
            .delete(&mut *remove_transaction, device_id)
            .await?
        {
            self.record_push_device_event
                .execute(
                    &mut remove_transaction,
                    removed_device.user_id,
                    removed_device.device_id,
                    PushDeviceChange::Removed(PushDeviceRemovalReason::Gone),
                    None,
                )
                .await?;
        }
        remove_transaction.commit().await?;
        Ok(())
    }
}
