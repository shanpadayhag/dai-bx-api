use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{message} at {path}")]
    ValidationAt { message: String, path: String },
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("{0}")]
    Conflict(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("the email or password is incorrect")]
    InvalidCredentials,
    #[error("forbidden")]
    Forbidden,
    #[error("content type must be application/json")]
    UnsupportedMediaType,
    #[error("too many attempts")]
    TooManyRequests { retry_after_seconds: u64 },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
}

#[derive(Serialize)]
struct LocatedErrorResponse<'a> {
    error: &'a str,
    path: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::TooManyRequests {
            retry_after_seconds,
        } = self
        {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after_seconds.max(1).to_string())],
                Json(ErrorResponse {
                    error: "too many attempts, try again later",
                }),
            )
                .into_response();
        }
        if let AppError::ValidationAt { message, path } = &self {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(LocatedErrorResponse {
                    error: message,
                    path,
                }),
            )
                .into_response();
        }
        let (status_code, safe_message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            AppError::Validation(validation_message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                validation_message.as_str(),
            ),
            AppError::ValidationAt { message, .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, message.as_str())
            }
            AppError::PayloadTooLarge(too_large_message) => {
                (StatusCode::PAYLOAD_TOO_LARGE, too_large_message.as_str())
            }
            AppError::Conflict(conflict_message) => {
                (StatusCode::CONFLICT, conflict_message.as_str())
            }
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "the email or password is incorrect",
            ),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::TooManyRequests { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts, try again later",
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_too_many_requests_with_429_and_a_retry_after_header() {
        let response = AppError::TooManyRequests {
            retry_after_seconds: 120,
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|header_value| header_value.to_str().ok()),
            Some("120")
        );
    }

    #[tokio::test]
    async fn answers_a_located_validation_error_with_its_path() {
        let response = AppError::ValidationAt {
            message: String::from("every task needs a name"),
            path: String::from("tasks[0].name"),
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response_body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .expect("the body should read");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response_body).expect("JSON"),
            serde_json::json!({ "error": "every task needs a name", "path": "tasks[0].name" })
        );
    }

    #[test]
    fn answers_invalid_credentials_with_401_and_one_generic_message() {
        let response = AppError::InvalidCredentials.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn never_sends_a_retry_after_of_zero() {
        let response = AppError::TooManyRequests {
            retry_after_seconds: 0,
        }
        .into_response();

        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|header_value| header_value.to_str().ok()),
            Some("1")
        );
    }
}
