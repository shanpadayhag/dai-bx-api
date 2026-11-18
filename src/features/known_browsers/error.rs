use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum KnownBrowserError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<KnownBrowserError> for AppError {
    fn from(known_browser_error: KnownBrowserError) -> Self {
        match known_browser_error {
            KnownBrowserError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
