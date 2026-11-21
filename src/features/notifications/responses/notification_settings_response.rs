use serde::Serialize;

use crate::features::notifications::entities::notification_settings::NotificationSettings;
use crate::features::notifications::entities::push_device::PushDevice;
use crate::features::notifications::responses::push_device_response::PushDeviceResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsResponse {
    pub application_server_key: String,
    pub check_ins: bool,
    pub schedule_runs: bool,
    pub blocks_starting: bool,
    pub quiet_outside_working_hours: bool,
    pub devices: Vec<PushDeviceResponse>,
}

impl NotificationSettingsResponse {
    pub fn new(
        application_server_key: &str,
        notification_settings: NotificationSettings,
        push_devices: Vec<PushDevice>,
    ) -> Self {
        Self {
            application_server_key: application_server_key.to_owned(),
            check_ins: notification_settings.check_ins,
            schedule_runs: notification_settings.schedule_runs,
            blocks_starting: notification_settings.blocks_starting,
            quiet_outside_working_hours: notification_settings.quiet_outside_working_hours,
            devices: push_devices
                .into_iter()
                .map(PushDeviceResponse::from)
                .collect(),
        }
    }
}
