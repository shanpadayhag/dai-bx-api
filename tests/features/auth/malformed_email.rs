use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{STRONG_PASSWORD, build_test_app, post_json, response_json};

#[sqlx::test]
async fn endpoints_that_take_an_email_return_422_for_a_malformed_one(db_pool: PgPool) {
    for (request_path, request_body) in [
        (
            "/auth/sign-in",
            json!({ "email": "not-an-email", "password": STRONG_PASSWORD }),
        ),
        (
            "/auth/verify-email/resend",
            json!({ "email": "not-an-email" }),
        ),
        ("/auth/password-reset", json!({ "email": "not-an-email" })),
    ] {
        let response = post_json(build_test_app(db_pool.clone()), request_path, request_body).await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{request_path}"
        );
        assert_eq!(
            response_json(response).await["error"],
            "enter a valid email address",
            "{request_path}"
        );
    }
}
