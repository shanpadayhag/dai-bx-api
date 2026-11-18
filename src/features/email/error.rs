use crate::core::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SmtpFailureKind {
    #[error("the connection timed out")]
    Timeout,
    #[error("the server refused it for now")]
    Transient,
    #[error("the server refused it")]
    Permanent,
    #[error("the connection failed")]
    Connection,
}

impl From<&lettre::transport::smtp::Error> for SmtpFailureKind {
    fn from(smtp_error: &lettre::transport::smtp::Error) -> Self {
        if smtp_error.is_timeout() {
            Self::Timeout
        } else if smtp_error.is_transient() {
            Self::Transient
        } else if smtp_error.is_permanent() {
            Self::Permanent
        } else {
            Self::Connection
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("recipient address is invalid")]
    InvalidRecipient,
    #[error("email could not be built: {0}")]
    Build(#[from] lettre::error::Error),
    #[error("email could not be sent: {0}")]
    Send(SmtpFailureKind),
}

impl From<lettre::transport::smtp::Error> for EmailError {
    fn from(smtp_error: lettre::transport::smtp::Error) -> Self {
        Self::Send(SmtpFailureKind::from(&smtp_error))
    }
}

impl From<EmailError> for AppError {
    fn from(email_error: EmailError) -> Self {
        AppError::Internal(email_error.into())
    }
}
