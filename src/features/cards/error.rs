use crate::core::error::AppError;
use crate::features::board_lists::error::BoardListError;
use crate::features::boards::error::BoardError;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::card_schedules::error::ScheduleError;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Debug, thiserror::Error)]
pub enum CardError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
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
            CardError::Conflict(conflict_message) => AppError::Conflict(conflict_message),
            CardError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}

impl From<CalendarEventError> for CardError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => CardError::Invalid(invalid_message),
            CalendarEventError::NotFound => CardError::NotFound,
            CalendarEventError::AlreadyRunning => {
                CardError::Invalid(String::from("this is already running"))
            }
            CalendarEventError::Database(database_error) => CardError::Database(database_error),
        }
    }
}

impl From<ScheduleError> for CardError {
    fn from(schedule_error: ScheduleError) -> Self {
        match schedule_error {
            ScheduleError::Invalid(invalid_message) => CardError::Invalid(invalid_message),
            ScheduleError::NotFound => CardError::NotFound,
            ScheduleError::Database(database_error) => CardError::Database(database_error),
        }
    }
}

impl From<TaskGroupError> for CardError {
    fn from(task_group_error: TaskGroupError) -> Self {
        match task_group_error {
            TaskGroupError::Invalid(invalid_message) => CardError::Invalid(invalid_message),
            TaskGroupError::NotFound => CardError::NotFound,
            TaskGroupError::Database(database_error) => CardError::Database(database_error),
        }
    }
}
