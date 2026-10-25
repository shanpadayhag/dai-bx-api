use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{build_test_app, get, patch_json_with_cookies, post_json};

#[sqlx::test]
async fn every_account_endpoint_that_needs_a_session_returns_401_without_one(db_pool: PgPool) {
    for request_path in ["/account", "/account/sessions"] {
        let response = get(build_test_app(db_pool.clone()), request_path).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "GET {request_path}"
        );
    }
    let patch_response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account",
        json!({ "theme": "dark" }),
        "",
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::UNAUTHORIZED,
        "PATCH /account"
    );
    for request_path in [
        "/account/password",
        "/account/email",
        "/account/recovery-codes",
        "/account/authenticator",
        "/account/authenticator/confirm",
        "/account/delete",
        "/account/sessions/end",
        "/account/sessions/end-all",
    ] {
        let response = post_json(build_test_app(db_pool.clone()), request_path, json!({})).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "POST {request_path}"
        );
    }
}
