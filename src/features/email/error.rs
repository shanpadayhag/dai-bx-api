use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("recipient address is invalid")]
    InvalidRecipient,
    #[error("email could not be built: {0}")]
    Build(#[from] lettre::error::Error),
    #[error("email could not be sent: {0}")]
    Send(#[from] lettre::transport::smtp::Error),
}

impl From<EmailError> for AppError {
    fn from(email_error: EmailError) -> Self {
        AppError::Internal(email_error.into())
    }
}
