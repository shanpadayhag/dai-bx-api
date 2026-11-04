use crate::core::error::AppError;
use crate::features::boards::error::BoardError;
use crate::features::calendar_events::error::CalendarEventError;

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

impl From<CalendarEventError> for BoardListError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => {
                BoardListError::Invalid(invalid_message)
            }
            CalendarEventError::NotFound => BoardListError::NotFound,
            CalendarEventError::AlreadyRunning => {
                BoardListError::Invalid(String::from("this is already running"))
            }
            CalendarEventError::Database(database_error) => {
                BoardListError::Database(database_error)
            }
        }
    }
}
