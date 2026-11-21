use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::request_context::RequestContext;
use crate::features::notifications::entities::push_device::PushDeviceRemovalReason;
use crate::features::notifications::error::NotificationError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::users::actions::find_user_by_id::FindUserById;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDeviceChange {
    Added,
    Removed(PushDeviceRemovalReason),
}

pub struct RecordPushDeviceEvent {
    find_user_by_id: FindUserById,
    record_security_event: RecordSecurityEvent,
}

impl Default for RecordPushDeviceEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordPushDeviceEvent {
    pub fn new() -> Self {
        Self {
            find_user_by_id: FindUserById::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        device_id: Uuid,
        push_device_change: PushDeviceChange,
        request_context: Option<&RequestContext>,
    ) -> Result<(), NotificationError> {
        let Some(device_owner) = self
            .find_user_by_id
            .execute(&mut *connection, user_id)
            .await?
        else {
            return Ok(());
        };
        let device_details =
            SecurityEventDetails::default().with_text("device_ref", &device_id.to_string())?;
        let (event_type, details) = match push_device_change {
            PushDeviceChange::Added => (SecurityEventType::PushDeviceAdded, device_details),
            PushDeviceChange::Removed(removal_reason) => (
                SecurityEventType::PushDeviceRemoved,
                device_details.with_text("reason", removal_reason.as_str())?,
            ),
        };
        self.record_security_event
            .execute(
                &mut *connection,
                &NewSecurityEvent {
                    event_type,
                    subject_ref: Some(device_owner.security_ref),
                    client_ip: request_context
                        .and_then(|request_context| request_context.client_ip),
                    user_agent: request_context
                        .and_then(|request_context| request_context.user_agent.clone()),
                    details,
                },
            )
            .await?;
        Ok(())
    }
}
