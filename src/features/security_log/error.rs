use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SecurityLogError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<SecurityLogError> for AppError {
    fn from(security_log_error: SecurityLogError) -> Self {
        match security_log_error {
            SecurityLogError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
