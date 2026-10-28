use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum CategoryError {
    #[error("{0}")]
    Invalid(String),
    #[error("category not found")]
    NotFound,
    #[error("you already have a category with that name")]
    NameTaken,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<CategoryError> for AppError {
    fn from(category_error: CategoryError) -> Self {
        match category_error {
            CategoryError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            CategoryError::NotFound => AppError::NotFound,
            CategoryError::NameTaken => AppError::Conflict(category_error.to_string()),
            CategoryError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
