use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Details(#[from] serde_json::Error),
}

impl From<ActivityError> for AppError {
    fn from(activity_error: ActivityError) -> Self {
        match activity_error {
            ActivityError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            ActivityError::Database(database_error) => AppError::Internal(database_error.into()),
            ActivityError::Details(details_error) => AppError::Internal(details_error.into()),
        }
    }
}
