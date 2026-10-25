use crate::core::error::AppError;
use crate::core::security::totp_secret_cipher::TotpCipherError;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    TotpCipher(#[from] TotpCipherError),
}

impl From<UserError> for AppError {
    fn from(user_error: UserError) -> Self {
        match user_error {
            UserError::Database(database_error) => AppError::Internal(database_error.into()),
            UserError::TotpCipher(cipher_error) => AppError::Internal(cipher_error.into()),
        }
    }
}
