use crate::core::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum CalendarEventError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("already running")]
    AlreadyRunning,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<CalendarEventError> for AppError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            CalendarEventError::NotFound => AppError::NotFound,
            CalendarEventError::AlreadyRunning => {
                AppError::Conflict(String::from("this is already running"))
            }
            CalendarEventError::Database(database_error) => {
                AppError::Internal(database_error.into())
            }
        }
    }
}
