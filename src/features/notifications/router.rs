use axum::Router;
use axum::routing::{delete, get, post};

use crate::core::state::AppState;
use crate::features::notifications::handlers::{
    delete_push_device, get_account_notifications, patch_account_notifications,
    post_push_device_test, post_push_devices,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/account/notifications",
            get(get_account_notifications::get_account_notifications)
                .patch(patch_account_notifications::patch_account_notifications),
        )
        .route("/push-devices", post(post_push_devices::post_push_devices))
        .route(
            "/push-devices/{device_id}",
            delete(delete_push_device::delete_push_device),
        )
        .route(
            "/push-devices/{device_id}/test",
            post(post_push_device_test::post_push_device_test),
        )
}
