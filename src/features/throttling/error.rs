use crate::core::error::AppError;
use crate::features::security_log::error::SecurityLogError;

#[derive(Debug, thiserror::Error)]
pub enum ThrottleError {
    #[error("blocked for {retry_after_seconds} seconds")]
    Blocked { retry_after_seconds: u64 },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<SecurityLogError> for ThrottleError {
    fn from(security_log_error: SecurityLogError) -> Self {
        match security_log_error {
            SecurityLogError::Database(database_error) => ThrottleError::Database(database_error),
        }
    }
}

impl From<ThrottleError> for AppError {
    fn from(throttle_error: ThrottleError) -> Self {
        match throttle_error {
            ThrottleError::Blocked {
                retry_after_seconds,
            } => AppError::TooManyRequests {
                retry_after_seconds,
            },
            ThrottleError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
