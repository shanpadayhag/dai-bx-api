use serde::Serialize;

use crate::features::notifications::entities::push_device::PushDevice;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushDeviceResponse {
    pub id: String,
    pub device_name: String,
    pub created_at: String,
    pub last_delivered_at: Option<String>,
}

impl From<PushDevice> for PushDeviceResponse {
    fn from(push_device: PushDevice) -> Self {
        Self {
            id: push_device.id.to_string(),
            device_name: push_device.device_name,
            created_at: push_device.created_at,
            last_delivered_at: push_device.last_delivered_at,
        }
    }
}
