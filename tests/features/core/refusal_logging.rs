use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    build_test_app, capture_logs, get, get_with_cookies, patch_json_with_cookies, send,
    sign_up_with_authenticator,
};

async fn read_security_ref(db_pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("SELECT security_ref FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(db_pool)
        .await
        .expect("reading the security reference should succeed")
}

#[sqlx::test]
async fn another_accounts_record_logs_the_route_and_account_without_the_id(db_pool: PgPool) {
    let board_owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &board_owner, "Private").await;
    let owned_board_id = board_id(&created_board);
    let other_security_ref = read_security_ref(&db_pool, &other_person.email).await;
    let (captured_logs, _subscriber_guard) = capture_logs();

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{owned_board_id}"),
        json!({ "name": "Taken" }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let refusal_lines = captured_logs.refusal_lines();
    assert_eq!(refusal_lines.len(), 1);
    assert_eq!(refusal_lines[0]["route"], "/boards/{board_id}");
    assert_eq!(refusal_lines[0]["method"], "PATCH");
    assert_eq!(refusal_lines[0]["status"], 404);
    assert_eq!(
        refusal_lines[0]["security_ref"],
        other_security_ref.to_string()
    );
    let log_text = captured_logs.text();
    assert!(!log_text.contains(&owned_board_id.to_string()));
    assert!(!log_text.contains("Taken"));
}

#[sqlx::test]
async fn a_signed_out_request_logs_no_account(db_pool: PgPool) {
    let (captured_logs, _subscriber_guard) = capture_logs();

    let response = get(build_test_app(db_pool), "/account").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let refusal_lines = captured_logs.refusal_lines();
    assert_eq!(refusal_lines.len(), 1);
    assert_eq!(refusal_lines[0]["route"], "/account");
    assert_eq!(refusal_lines[0]["status"], 401);
    assert!(refusal_lines[0].get("security_ref").is_none());
}

#[sqlx::test]
async fn an_unknown_path_is_logged_without_the_path(db_pool: PgPool) {
    let (captured_logs, _subscriber_guard) = capture_logs();

    let response = get(
        build_test_app(db_pool),
        "/no-such/private-value?token=hidden",
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(captured_logs.refusal_lines()[0]["route"], "unmatched");
    let log_text = captured_logs.text();
    assert!(!log_text.contains("private-value"));
    assert!(!log_text.contains("hidden"));
}

#[sqlx::test]
async fn a_cross_site_request_is_logged(db_pool: PgPool) {
    let (captured_logs, _subscriber_guard) = capture_logs();
    let request = Request::post("/auth/sign-in")
        .header(header::CONTENT_TYPE, "application/json")
        .header("sec-fetch-site", "cross-site")
        .body(Body::from("{}"))
        .expect("test request should build");

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let refusal_lines = captured_logs.refusal_lines();
    assert_eq!(refusal_lines[0]["route"], "/auth/sign-in");
    assert_eq!(refusal_lines[0]["status"], 403);
}

#[sqlx::test]
async fn an_allowed_request_logs_no_refusal(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (captured_logs, _subscriber_guard) = capture_logs();

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(captured_logs.refusal_lines().is_empty());
}
