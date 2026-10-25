use uuid::Uuid;

pub const REMEMBERED_DEVICE_VALID_FOR_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const REMEMBERED_DEVICE_COOKIE_NAME: &str = "__Host-daibx_device";

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct RememberedDevice {
    pub id: Uuid,
    pub user_id: Uuid,
}
