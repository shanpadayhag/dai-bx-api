use crate::core::error::AppError;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::cards::error::CardError;
use crate::features::tasks::error::TaskError;

#[derive(Debug, thiserror::Error)]
pub enum TrackingError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("already running")]
    AlreadyRunning,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<TaskError> for TrackingError {
    fn from(task_error: TaskError) -> Self {
        match task_error {
            TaskError::Invalid(invalid_message) => TrackingError::Invalid(invalid_message),
            TaskError::InvalidAt { message, .. } => TrackingError::Invalid(message),
            TaskError::NotFound => TrackingError::NotFound,
            TaskError::Database(database_error) => TrackingError::Database(database_error),
        }
    }
}

impl From<CardError> for TrackingError {
    fn from(card_error: CardError) -> Self {
        match card_error {
            CardError::Invalid(invalid_message) => TrackingError::Invalid(invalid_message),
            CardError::NotFound => TrackingError::NotFound,
            CardError::Conflict(conflict_message) => TrackingError::Invalid(conflict_message),
            CardError::Database(database_error) => TrackingError::Database(database_error),
        }
    }
}

impl From<CalendarEventError> for TrackingError {
    fn from(calendar_event_error: CalendarEventError) -> Self {
        match calendar_event_error {
            CalendarEventError::Invalid(invalid_message) => TrackingError::Invalid(invalid_message),
            CalendarEventError::NotFound => TrackingError::NotFound,
            CalendarEventError::AlreadyRunning => TrackingError::AlreadyRunning,
            CalendarEventError::Database(database_error) => TrackingError::Database(database_error),
        }
    }
}

impl From<TrackingError> for AppError {
    fn from(tracking_error: TrackingError) -> Self {
        match tracking_error {
            TrackingError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            TrackingError::NotFound => AppError::NotFound,
            TrackingError::AlreadyRunning => {
                AppError::Conflict(String::from("this is already running"))
            }
            TrackingError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
