use crate::core::error::AppError;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Debug, thiserror::Error)]
pub enum TaskGroupError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<TaskGroupError> for AppError {
    fn from(task_group_error: TaskGroupError) -> Self {
        match task_group_error {
            TaskGroupError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            TaskGroupError::NotFound => AppError::NotFound,
            TaskGroupError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}

impl From<CalendarEventError> for TaskGroupError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => {
                TaskGroupError::Invalid(invalid_message)
            }
            CalendarEventError::NotFound => TaskGroupError::NotFound,
            CalendarEventError::AlreadyRunning => {
                TaskGroupError::Invalid(String::from("this is already running"))
            }
            CalendarEventError::Database(database_error) => {
                TaskGroupError::Database(database_error)
            }
        }
    }
}
