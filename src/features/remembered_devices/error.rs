use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum RememberedDeviceError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<RememberedDeviceError> for AppError {
    fn from(remembered_device_error: RememberedDeviceError) -> Self {
        match remembered_device_error {
            RememberedDeviceError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
