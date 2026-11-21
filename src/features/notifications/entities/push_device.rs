use uuid::Uuid;

use crate::features::notifications::entities::push_device_keys::PushDeviceKeys;
use crate::features::notifications::entities::push_endpoint::PushEndpoint;

pub const MAX_PUSH_DEVICES_PER_USER: i64 = 10;
pub const MAX_DEVICE_NAME_CHARS: usize = 60;
pub const MAX_DEVICE_TIME_ZONE_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDevice {
    pub id: Uuid,
    pub user_id: Uuid,
    pub endpoint: String,
    pub keys: PushDeviceKeys,
    pub device_name: String,
    pub time_zone: String,
    pub created_at: String,
    pub last_delivered_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPushDevice {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub endpoint: PushEndpoint,
    pub keys: PushDeviceKeys,
    pub device_name: String,
    pub time_zone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPushDevice {
    pub push_device: PushDevice,
    pub is_new: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDeviceRemovalReason {
    ByHand,
    SessionEnded,
    Gone,
    MovedToAnotherAccount,
}

impl PushDeviceRemovalReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PushDeviceRemovalReason::ByHand => "by_hand",
            PushDeviceRemovalReason::SessionEnded => "session_ended",
            PushDeviceRemovalReason::Gone => "gone",
            PushDeviceRemovalReason::MovedToAnotherAccount => "moved_to_another_account",
        }
    }
}
