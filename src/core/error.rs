use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("content type must be application/json")]
    UnsupportedMediaType,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status_code, safe_message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            AppError::Validation(validation_message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                validation_message.as_str(),
            ),
            AppError::Conflict(conflict_message) => {
                (StatusCode::CONFLICT, conflict_message.as_str())
            }
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "content type must be application/json",
            ),
            AppError::Internal(internal_error) => {
                tracing::error!(error = ?internal_error, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
            }
        };
        (
            status_code,
            Json(ErrorResponse {
                error: safe_message,
            }),
        )
            .into_response()
    }
}
