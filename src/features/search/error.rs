use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<SearchError> for AppError {
    fn from(search_error: SearchError) -> Self {
        match search_error {
            SearchError::Invalid(invalid_message) => {
                AppError::Validation(String::from(invalid_message))
            }
            SearchError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
