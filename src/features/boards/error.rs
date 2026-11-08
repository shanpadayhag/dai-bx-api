use crate::core::error::AppError;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::card_schedules::error::ScheduleError;

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

impl From<CalendarEventError> for BoardError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => BoardError::Invalid(invalid_message),
            CalendarEventError::NotFound => BoardError::NotFound,
            CalendarEventError::AlreadyRunning => {
                BoardError::Invalid(String::from("this is already running"))
            }
            CalendarEventError::Database(database_error) => BoardError::Database(database_error),
        }
    }
}

impl From<ScheduleError> for BoardError {
    fn from(schedule_error: ScheduleError) -> Self {
        match schedule_error {
            ScheduleError::Invalid(invalid_message) => BoardError::Invalid(invalid_message),
            ScheduleError::NotFound => BoardError::NotFound,
            ScheduleError::Database(database_error) => BoardError::Database(database_error),
        }
    }
}
