use crate::core::error::AppError;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::security_log::entities::security_event::ForbiddenDetail;
use crate::features::security_log::error::SecurityLogError;
use crate::features::throttling::error::ThrottleError;
use crate::features::users::error::UserError;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("this account already has the most devices allowed")]
    DeviceLimit,
    #[error("the push service no longer knows this device")]
    DeviceGone,
    #[error("the push service refused the notification")]
    PushFailed,
    #[error("account no longer exists")]
    NotSignedIn,
    #[error(transparent)]
    Throttle(#[from] ThrottleError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

macro_rules! notification_error_from_internal {
    ($($internal_error_type:ty),+ $(,)?) => {
        $(impl From<$internal_error_type> for NotificationError {
            fn from(internal_error: $internal_error_type) -> Self {
                NotificationError::Internal(internal_error.into())
            }
        })+
    };
}

notification_error_from_internal!(
    CalendarEventError,
    ForbiddenDetail,
    SecurityLogError,
    UserError,
    sqlx::Error,
);

impl From<NotificationError> for AppError {
    fn from(notification_error: NotificationError) -> Self {
        match notification_error {
            NotificationError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            NotificationError::NotFound => AppError::NotFound,
            NotificationError::DeviceLimit => AppError::Conflict(String::from(
                "this account already gets notifications on 10 devices",
            )),
            NotificationError::DeviceGone => AppError::Gone(String::from(
                "this device no longer gets notifications, turn them on again",
            )),
            NotificationError::PushFailed => AppError::BadGateway(String::from(
                "the push service did not accept the notification",
            )),
            NotificationError::NotSignedIn => AppError::Unauthorized,
            NotificationError::Throttle(throttle_error) => throttle_error.into(),
            NotificationError::Internal(internal_error) => AppError::Internal(internal_error),
        }
    }
}
