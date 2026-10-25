use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryCodeError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<RecoveryCodeError> for AppError {
    fn from(recovery_code_error: RecoveryCodeError) -> Self {
        match recovery_code_error {
            RecoveryCodeError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
