use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum AuthChallengeError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<AuthChallengeError> for AppError {
    fn from(auth_challenge_error: AuthChallengeError) -> Self {
        match auth_challenge_error {
            AuthChallengeError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
