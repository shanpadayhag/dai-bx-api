use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum EmailTokenError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<EmailTokenError> for AppError {
    fn from(email_token_error: EmailTokenError) -> Self {
        match email_token_error {
            EmailTokenError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
