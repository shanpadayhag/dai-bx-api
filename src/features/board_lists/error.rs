use crate::core::error::AppError;
use crate::features::boards::error::BoardError;

#[derive(Debug, thiserror::Error)]
pub enum BoardListError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<BoardError> for BoardListError {
    fn from(board_error: BoardError) -> Self {
        match board_error {
            BoardError::Invalid(invalid_message) => BoardListError::Invalid(invalid_message),
            BoardError::NotFound => BoardListError::NotFound,
            BoardError::Database(database_error) => BoardListError::Database(database_error),
        }
    }
}

impl From<BoardListError> for AppError {
    fn from(board_list_error: BoardListError) -> Self {
        match board_list_error {
            BoardListError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            BoardListError::NotFound => AppError::NotFound,
            BoardListError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
