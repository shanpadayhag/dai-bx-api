use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::support::{build_test_app, get, post_json, send};

fn assert_protective_headers(response: &Response<Body>, expected_status: StatusCode) {
    assert_eq!(response.status(), expected_status);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store")),
        "{expected_status} should not be cached"
    );
    assert_eq!(
        response.headers().get(header::X_CONTENT_TYPE_OPTIONS),
        Some(&header::HeaderValue::from_static("nosniff")),
        "{expected_status} should not be sniffed"
    );
}

#[sqlx::test]
async fn a_successful_response_carries_both_headers(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/health").await;

    assert_protective_headers(&response, StatusCode::OK);
}

#[sqlx::test]
async fn a_signed_out_refusal_carries_both_headers(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/account").await;

    assert_protective_headers(&response, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn an_unknown_path_carries_both_headers(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/no/such/path").await;

    assert_protective_headers(&response, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_wrong_method_carries_both_headers(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/auth/sign-in").await;

    assert_protective_headers(&response, StatusCode::METHOD_NOT_ALLOWED);
}

#[sqlx::test]
async fn a_cross_site_refusal_carries_both_headers(db_pool: PgPool) {
    let request = Request::post("/auth/sign-in")
        .header(header::CONTENT_TYPE, "application/json")
        .header("sec-fetch-site", "cross-site")
        .body(Body::from("{}"))
        .expect("test request should build");

    let response = send(build_test_app(db_pool), request).await;

    assert_protective_headers(&response, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn an_oversized_body_carries_both_headers(db_pool: PgPool) {
    let oversized_text = "x".repeat(70 * 1024);

    let response = post_json(
        build_test_app(db_pool),
        "/auth/sign-in",
        json!({ "email": oversized_text, "password": "unused" }),
    )
    .await;

    assert_protective_headers(&response, StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test]
async fn a_timed_out_request_carries_both_headers(db_pool: PgPool) {
    let mut blocking_transaction = db_pool.begin().await.expect("a transaction");
    sqlx::query("LOCK TABLE auth_throttles IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocking_transaction)
        .await
        .expect("locking the table should succeed");

    let response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-in",
        json!({ "email": "person@daibx.test", "password": "unused password" }),
    )
    .await;

    assert_protective_headers(&response, StatusCode::REQUEST_TIMEOUT);
    blocking_transaction
        .rollback()
        .await
        .expect("releasing the lock should succeed");
}
