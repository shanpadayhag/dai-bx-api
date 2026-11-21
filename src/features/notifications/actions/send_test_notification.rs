use reqwest::Client;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::config::vapid_settings::VapidSettings;
use crate::features::notifications::actions::remove_gone_push_device::RemoveGonePushDevice;
use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::data_sources::web_push::web_push_source::{
    PushOutcome, WebPushSource,
};
use crate::features::notifications::entities::push_message::PushMessage;
use crate::features::notifications::error::NotificationError;
use crate::features::throttling::actions::allow_test_push::AllowTestPush;

pub struct SendTestNotification<'a> {
    db_pool: &'a PgPool,
    postgres_push_device_source: PostgresPushDeviceSource,
    web_push_source: WebPushSource<'a>,
    allow_test_push: AllowTestPush<'a>,
    remove_gone_push_device: RemoveGonePushDevice<'a>,
}

impl<'a> SendTestNotification<'a> {
    pub fn new(
        db_pool: &'a PgPool,
        http_client: &'a Client,
        vapid_settings: &'a VapidSettings,
    ) -> Self {
        Self {
            db_pool,
            postgres_push_device_source: PostgresPushDeviceSource,
            web_push_source: WebPushSource::new(http_client, vapid_settings),
            allow_test_push: AllowTestPush::new(db_pool),
            remove_gone_push_device: RemoveGonePushDevice::new(db_pool),
        }
    }

    pub async fn execute(&self, user_id: Uuid, device_id: Uuid) -> Result<(), NotificationError> {
        let push_device = self
            .postgres_push_device_source
            .find_owned(self.db_pool, user_id, device_id)
            .await?
            .ok_or(NotificationError::NotFound)?;
        self.allow_test_push.execute(push_device.id).await?;
        let push_outcome = self
            .web_push_source
            .send(
                &push_device.endpoint,
                &push_device.keys,
                &PushMessage::test(),
            )
            .await;
        match push_outcome {
            Ok(PushOutcome::Delivered) => {
                self.postgres_push_device_source
                    .mark_delivered(self.db_pool, push_device.id)
                    .await?;
                Ok(())
            }
            Ok(PushOutcome::Gone) => {
                self.remove_gone_push_device.execute(push_device.id).await?;
                Err(NotificationError::DeviceGone)
            }
            Ok(PushOutcome::Refused(refused_status)) => {
                tracing::warn!(device_id = %push_device.id, refused_status, "test notification refused");
                Err(NotificationError::PushFailed)
            }
            Err(web_push_error) => {
                tracing::warn!(device_id = %push_device.id, error = %web_push_error, "test notification failed");
                Err(NotificationError::PushFailed)
            }
        }
    }
}
