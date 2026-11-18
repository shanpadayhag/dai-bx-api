use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    ORIGIN_SECRET_HEADER, TEST_ORIGIN_SECRET, build_test_app, capture_logs, response_json, send,
    send_direct,
};

const FORWARDED_CLIENT_IP: &str = "180.191.77.129";

fn account_request() -> axum::http::request::Builder {
    Request::get("/account").header("x-vercel-forwarded-for", FORWARDED_CLIENT_IP)
}

#[sqlx::test]
async fn a_direct_call_without_the_origin_secret_is_refused(db_pool: PgPool) {
    let request = account_request().body(Body::empty()).unwrap();

    let response = send_direct(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "forbidden" })
    );
}

#[sqlx::test]
async fn a_direct_call_with_a_wrong_origin_secret_is_refused(db_pool: PgPool) {
    let request = account_request()
        .header(ORIGIN_SECRET_HEADER, format!("{TEST_ORIGIN_SECRET}x"))
        .body(Body::empty())
        .unwrap();

    let response = send_direct(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn a_direct_call_to_an_unknown_address_is_refused(db_pool: PgPool) {
    let request = Request::get("/not-a-route").body(Body::empty()).unwrap();

    let response = send_direct(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn a_call_carrying_the_origin_secret_reaches_the_route(db_pool: PgPool) {
    let request = account_request().body(Body::empty()).unwrap();

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn health_answers_without_the_origin_secret(db_pool: PgPool) {
    let request = Request::get("/health").body(Body::empty()).unwrap();

    let response = send_direct(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn a_refused_direct_call_is_logged_and_never_cached(db_pool: PgPool) {
    let (captured_logs, _subscriber_guard) = capture_logs();
    let request = account_request().body(Body::empty()).unwrap();

    let response = send_direct(build_test_app(db_pool), request).await;

    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let refusal_lines = captured_logs.refusal_lines();
    assert_eq!(refusal_lines.len(), 1);
    assert_eq!(refusal_lines[0]["status"], 403);
}

#[sqlx::test]
async fn a_direct_call_with_a_faked_client_ip_never_reaches_a_limit(db_pool: PgPool) {
    let test_app = build_test_app(db_pool.clone());
    for attempt_number in 0..3 {
        let request = Request::post("/auth/password-reset")
            .header("sec-fetch-site", "same-origin")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                "x-vercel-forwarded-for",
                format!("203.0.113.{attempt_number}"),
            )
            .body(Body::from(
                json!({ "email": "someone@example.com" }).to_string(),
            ))
            .unwrap();

        let response = send_direct(test_app.clone(), request).await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    let throttle_row_count: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_throttles")
        .fetch_one(&db_pool)
        .await
        .unwrap();
    let security_event_count: i64 = sqlx::query_scalar("SELECT count(*) FROM security_events")
        .fetch_one(&db_pool)
        .await
        .unwrap();
    assert_eq!(throttle_row_count, 0);
    assert_eq!(security_event_count, 0);
}
