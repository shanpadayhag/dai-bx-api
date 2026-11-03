use crate::core::error::AppError;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::error::BoardError;

#[derive(Debug, thiserror::Error)]
pub enum CardError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<BoardError> for CardError {
    fn from(board_error: BoardError) -> Self {
        match board_error {
            BoardError::Invalid(invalid_message) => CardError::Invalid(invalid_message),
            BoardError::NotFound => CardError::NotFound,
            BoardError::Database(database_error) => CardError::Database(database_error),
        }
    }
}

impl From<BoardListError> for CardError {
    fn from(board_list_error: BoardListError) -> Self {
        match board_list_error {
            BoardListError::Invalid(invalid_message) => CardError::Invalid(invalid_message),
            BoardListError::NotFound => CardError::NotFound,
            BoardListError::Database(database_error) => CardError::Database(database_error),
        }
    }
}

impl From<CardError> for AppError {
    fn from(card_error: CardError) -> Self {
        match card_error {
            CardError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            CardError::NotFound => AppError::NotFound,
            CardError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
