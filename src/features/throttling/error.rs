use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum ThrottleError {
    #[error("blocked for {retry_after_seconds} seconds")]
    Blocked { retry_after_seconds: u64 },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
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
