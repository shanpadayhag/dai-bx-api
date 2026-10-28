use crate::core::error::AppError;

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
