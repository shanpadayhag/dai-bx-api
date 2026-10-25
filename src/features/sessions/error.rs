use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<SessionError> for AppError {
    fn from(session_error: SessionError) -> Self {
        match session_error {
            SessionError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
