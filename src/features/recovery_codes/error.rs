use crate::core::error::AppError;
use crate::core::security::password_hash::PasswordHashError;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryCodeError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Hashing(#[from] PasswordHashError),
}

impl From<RecoveryCodeError> for AppError {
    fn from(recovery_code_error: RecoveryCodeError) -> Self {
        match recovery_code_error {
            RecoveryCodeError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
            RecoveryCodeError::Hashing(hashing_error) => AppError::Internal(hashing_error.into()),
        }
    }
}
