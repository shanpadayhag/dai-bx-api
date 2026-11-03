use crate::core::error::AppError;
use crate::features::boards::error::BoardError;

#[derive(Debug, thiserror::Error)]
pub enum BoardPropertyError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("this board already has a property with that name")]
    PropertyNameTaken,
    #[error("this property already has an option with that name")]
    OptionNameTaken,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<BoardError> for BoardPropertyError {
    fn from(board_error: BoardError) -> Self {
        match board_error {
            BoardError::Invalid(invalid_message) => BoardPropertyError::Invalid(invalid_message),
            BoardError::NotFound => BoardPropertyError::NotFound,
            BoardError::Database(database_error) => BoardPropertyError::Database(database_error),
        }
    }
}

impl From<BoardPropertyError> for AppError {
    fn from(board_property_error: BoardPropertyError) -> Self {
        match board_property_error {
            BoardPropertyError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            BoardPropertyError::NotFound => AppError::NotFound,
            BoardPropertyError::PropertyNameTaken | BoardPropertyError::OptionNameTaken => {
                AppError::Conflict(board_property_error.to_string())
            }
            BoardPropertyError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
