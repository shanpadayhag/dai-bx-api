use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<HealthError> for AppError {
    fn from(health_error: HealthError) -> Self {
        match health_error {
            HealthError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
