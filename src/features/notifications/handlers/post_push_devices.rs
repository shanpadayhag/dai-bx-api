use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::recurrence::parse_time_zone;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::notifications::actions::register_push_device::RegisterPushDevice;
use crate::features::notifications::entities::push_device::{
    MAX_DEVICE_NAME_CHARS, MAX_DEVICE_TIME_ZONE_CHARS, NewPushDevice,
};
use crate::features::notifications::entities::push_device_keys::{
    InvalidPushDeviceKeys, PushDeviceKeys,
};
use crate::features::notifications::entities::push_endpoint::PushEndpoint;
use crate::features::notifications::responses::push_device_response::PushDeviceResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostPushDeviceRequest {
    pub endpoint: String,
    pub keys: PushDeviceKeysRequest,
    pub device_name: String,
    pub time_zone: String,
}

#[derive(Deserialize)]
pub struct PushDeviceKeysRequest {
    pub p256dh: String,
    pub auth: String,
}

fn validate_device_name(device_name_text: &str) -> Result<String, AppError> {
    let trimmed_device_name = device_name_text.trim();
    let device_name_char_count = trimmed_device_name.chars().count();
    if device_name_char_count == 0
        || device_name_char_count > MAX_DEVICE_NAME_CHARS
        || trimmed_device_name.chars().any(char::is_control)
    {
        return Err(AppError::ValidationAt {
            message: String::from("the device name is 1 to 60 characters"),
            path: String::from("deviceName"),
        });
    }
    Ok(trimmed_device_name.to_owned())
}

fn validate_time_zone(time_zone_text: &str) -> Result<String, AppError> {
    if time_zone_text.chars().count() > MAX_DEVICE_TIME_ZONE_CHARS
        || parse_time_zone(time_zone_text).is_none()
    {
        return Err(AppError::ValidationAt {
            message: String::from("the time zone is a name like Asia/Manila"),
            path: String::from("timeZone"),
        });
    }
    Ok(time_zone_text.to_owned())
}

pub async fn post_push_devices(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_context: RequestContext,
    Json(post_request): Json<PostPushDeviceRequest>,
) -> Result<(StatusCode, Json<PushDeviceResponse>), AppError> {
    let endpoint =
        PushEndpoint::parse(&post_request.endpoint).map_err(|_| AppError::ValidationAt {
            message: String::from("the endpoint must be an https address on a known push service"),
            path: String::from("endpoint"),
        })?;
    let keys = PushDeviceKeys::parse(&post_request.keys.p256dh, &post_request.keys.auth).map_err(
        |invalid_keys| AppError::ValidationAt {
            message: invalid_keys.to_string(),
            path: String::from(match invalid_keys {
                InvalidPushDeviceKeys::P256dh => "keys.p256dh",
                InvalidPushDeviceKeys::AuthSecret => "keys.auth",
            }),
        },
    )?;
    let new_push_device = NewPushDevice {
        user_id: write_session.user_id,
        session_id: write_session.session_id,
        endpoint,
        keys,
        device_name: validate_device_name(&post_request.device_name)?,
        time_zone: validate_time_zone(&post_request.time_zone)?,
    };
    let registered_device = RegisterPushDevice::new(&app_state.db_pool)
        .execute(&new_push_device, &request_context)
        .await?;
    let response_status = if registered_device.is_new {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        response_status,
        Json(PushDeviceResponse::from(registered_device.push_device)),
    ))
}
