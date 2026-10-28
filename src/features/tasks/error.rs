use crate::core::error::AppError;
use crate::features::task_groups::error::TaskGroupError;
use crate::features::tasks::entities::group_import::ImportProblem;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("{0}")]
    Invalid(String),
    #[error("{message} at {path}")]
    InvalidAt { message: String, path: String },
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<TaskGroupError> for TaskError {
    fn from(task_group_error: TaskGroupError) -> Self {
        match task_group_error {
            TaskGroupError::Invalid(invalid_message) => TaskError::Invalid(invalid_message),
            TaskGroupError::NotFound => TaskError::NotFound,
            TaskGroupError::Database(database_error) => TaskError::Database(database_error),
        }
    }
}

impl From<TaskError> for AppError {
    fn from(task_error: TaskError) -> Self {
        match task_error {
            TaskError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            TaskError::InvalidAt { message, path } => AppError::ValidationAt { message, path },
            TaskError::NotFound => AppError::NotFound,
            TaskError::Database(database_error) => AppError::Internal(database_error.into()),
        }
    }
}

impl From<ImportProblem> for TaskError {
    fn from(import_problem: ImportProblem) -> Self {
        TaskError::InvalidAt {
            message: import_problem.message.to_owned(),
            path: import_problem.path,
        }
    }
}
