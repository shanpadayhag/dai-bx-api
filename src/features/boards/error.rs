use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<BoardError> for AppError {
    fn from(board_error: BoardError) -> Self {
        match board_error {
            BoardError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            BoardError::NotFound => AppError::NotFound,
            BoardError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
