use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_context::RequestContext;
use crate::features::notifications::actions::record_push_device_event::{
    PushDeviceChange, RecordPushDeviceEvent,
};
use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::entities::push_device::PushDeviceRemovalReason;
use crate::features::notifications::error::NotificationError;

pub struct RemovePushDevice<'a> {
    db_pool: &'a PgPool,
    postgres_push_device_source: PostgresPushDeviceSource,
    record_push_device_event: RecordPushDeviceEvent,
}

impl<'a> RemovePushDevice<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_push_device_source: PostgresPushDeviceSource,
            record_push_device_event: RecordPushDeviceEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        request_context: &RequestContext,
    ) -> Result<(), NotificationError> {
        let mut remove_transaction = self.db_pool.begin().await?;
        let is_removed = self
            .postgres_push_device_source
            .delete_owned(&mut *remove_transaction, user_id, device_id)
            .await?;
        if !is_removed {
            return Err(NotificationError::NotFound);
        }
        self.record_push_device_event
            .execute(
                &mut remove_transaction,
                user_id,
                device_id,
                PushDeviceChange::Removed(PushDeviceRemovalReason::ByHand),
                Some(request_context),
            )
            .await?;
        remove_transaction.commit().await?;
        Ok(())
    }
}
