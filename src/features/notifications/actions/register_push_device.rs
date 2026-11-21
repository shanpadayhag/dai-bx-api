use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::features::notifications::actions::record_push_device_event::{
    PushDeviceChange, RecordPushDeviceEvent,
};
use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::entities::push_device::{
    MAX_PUSH_DEVICES_PER_USER, NewPushDevice, PushDeviceRemovalReason, RegisteredPushDevice,
};
use crate::features::notifications::error::NotificationError;

pub struct RegisterPushDevice<'a> {
    db_pool: &'a PgPool,
    postgres_push_device_source: PostgresPushDeviceSource,
    record_push_device_event: RecordPushDeviceEvent,
}

impl<'a> RegisterPushDevice<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_push_device_source: PostgresPushDeviceSource,
            record_push_device_event: RecordPushDeviceEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        new_push_device: &NewPushDevice,
        request_context: &RequestContext,
    ) -> Result<RegisteredPushDevice, NotificationError> {
        let mut register_transaction = self.db_pool.begin().await?;
        let is_owner_present = self
            .postgres_push_device_source
            .lock_owner(&mut *register_transaction, new_push_device.user_id)
            .await?;
        if !is_owner_present {
            return Err(NotificationError::NotSignedIn);
        }
        let endpoint_owner = self
            .postgres_push_device_source
            .find_endpoint_owner(
                &mut *register_transaction,
                new_push_device.endpoint.as_str(),
            )
            .await?;
        if let Some(endpoint_owner) = endpoint_owner {
            if endpoint_owner.user_id == new_push_device.user_id {
                let updated_device = self
                    .postgres_push_device_source
                    .update_registration(
                        &mut *register_transaction,
                        endpoint_owner.device_id,
                        new_push_device,
                    )
                    .await?;
                register_transaction.commit().await?;
                return Ok(RegisteredPushDevice {
                    push_device: updated_device,
                    is_new: false,
                });
            }
            self.postgres_push_device_source
                .delete(&mut *register_transaction, endpoint_owner.device_id)
                .await?;
            self.record_push_device_event
                .execute(
                    &mut register_transaction,
                    endpoint_owner.user_id,
                    endpoint_owner.device_id,
                    PushDeviceChange::Removed(PushDeviceRemovalReason::MovedToAnotherAccount),
                    None,
                )
                .await?;
        }
        let existing_device_count = self
            .postgres_push_device_source
            .count_for_user(&mut *register_transaction, new_push_device.user_id)
            .await?;
        if existing_device_count >= MAX_PUSH_DEVICES_PER_USER {
            return Err(NotificationError::DeviceLimit);
        }
        let inserted_device = self
            .postgres_push_device_source
            .insert(&mut *register_transaction, new_push_device)
            .await?;
        self.record_push_device_event
            .execute(
                &mut register_transaction,
                new_push_device.user_id,
                inserted_device.id,
                PushDeviceChange::Added,
                Some(request_context),
            )
            .await?;
        register_transaction.commit().await?;
        Ok(RegisteredPushDevice {
            push_device: inserted_device,
            is_new: true,
        })
    }
}
