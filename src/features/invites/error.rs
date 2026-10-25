use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    #[error("invite code is invalid or expired")]
    InvalidCode,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<InviteError> for AppError {
    fn from(invite_error: InviteError) -> Self {
        match invite_error {
            InviteError::InvalidCode => {
                AppError::Validation(String::from("this invite code is invalid or has expired"))
            }
            InviteError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
