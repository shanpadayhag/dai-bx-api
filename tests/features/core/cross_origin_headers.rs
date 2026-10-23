use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::PgPool;

use crate::support::{build_test_app, send};

#[sqlx::test]
async fn answers_a_cross_origin_get_without_cors_headers(db_pool: PgPool) {
    let request = Request::get("/health")
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .expect("test request should build");

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[sqlx::test]
async fn never_approves_a_cors_preflight(db_pool: PgPool) {
    let request = Request::options("/health")
        .header("origin", "https://evil.example")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .expect("test request should build");

    let response = send(build_test_app(db_pool), request).await;

    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}
