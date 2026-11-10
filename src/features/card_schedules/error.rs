use crate::core::error::AppError;
use crate::features::cards::error::CardError;

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<CardError> for ScheduleError {
    fn from(card_error: CardError) -> Self {
        match card_error {
            CardError::Invalid(invalid_message) => ScheduleError::Invalid(invalid_message),
            CardError::NotFound => ScheduleError::NotFound,
            CardError::Conflict(conflict_message) => ScheduleError::Invalid(conflict_message),
            CardError::Database(database_error) => ScheduleError::Database(database_error),
        }
    }
}

impl From<ScheduleError> for AppError {
    fn from(schedule_error: ScheduleError) -> Self {
        match schedule_error {
            ScheduleError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            ScheduleError::NotFound => AppError::NotFound,
            ScheduleError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
