use crate::core::error::AppError;
use crate::features::account::error::AccountError;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Debug, thiserror::Error)]
pub enum PlanningError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    CalendarEvent(#[from] CalendarEventError),
    #[error(transparent)]
    TaskGroup(#[from] TaskGroupError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<PlanningError> for AppError {
    fn from(planning_error: PlanningError) -> Self {
        match planning_error {
            PlanningError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            PlanningError::NotFound => AppError::NotFound,
            PlanningError::Account(account_error) => account_error.into(),
            PlanningError::CalendarEvent(calendar_event_error) => calendar_event_error.into(),
            PlanningError::TaskGroup(task_group_error) => task_group_error.into(),
            PlanningError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}
