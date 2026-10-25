use axum::http::{StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, count_emails_after_a_pause, create_invite_code,
    latest_link_token, post_json, response_json, unique_email,
};

async fn sign_up(db_pool: &PgPool, email: &str) {
    let response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        json!({
            "inviteCode": create_invite_code(db_pool).await,
            "name": "Shan Padayhag",
            "email": email,
            "password": STRONG_PASSWORD,
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

async fn resend(db_pool: &PgPool, email: &str) -> axum::http::Response<axum::body::Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email/resend",
        json!({ "email": email }),
    )
    .await
}

#[sqlx::test]
async fn resend_returns_202_with_a_fresh_link_that_replaces_the_old_one(db_pool: PgPool) {
    let email = unique_email();
    sign_up(&db_pool, &email).await;
    let first_link_token = latest_link_token(&email, 1).await;

    let response = resend(&db_pool, &email).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(response).await["status"],
        "verification_email_sent"
    );
    let fresh_link_token = latest_link_token(&email, 2).await;
    assert_ne!(fresh_link_token, first_link_token);
    let old_link_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email",
        json!({ "token": first_link_token }),
    )
    .await;
    assert_eq!(old_link_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let fresh_link_response = post_json(
        build_test_app(db_pool),
        "/auth/verify-email",
        json!({ "token": fresh_link_token }),
    )
    .await;
    assert_eq!(fresh_link_response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn resend_answers_an_unknown_email_the_same_way_and_sends_nothing(db_pool: PgPool) {
    let unknown_email = unique_email();

    let response = resend(&db_pool, &unknown_email).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(response).await["status"],
        "verification_email_sent"
    );
    assert_eq!(count_emails_after_a_pause(&unknown_email).await, 0);
}

#[sqlx::test]
async fn resend_sends_nothing_for_an_already_verified_email(db_pool: PgPool) {
    let email = unique_email();
    sign_up(&db_pool, &email).await;
    let link_token = latest_link_token(&email, 1).await;
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;

    let response = resend(&db_pool, &email).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(count_emails_after_a_pause(&email).await, 1);
}

#[sqlx::test]
async fn resend_returns_429_after_three_requests_an_hour(db_pool: PgPool) {
    let email = unique_email();
    for _ in 0..3 {
        assert_eq!(
            resend(&db_pool, &email).await.status(),
            StatusCode::ACCEPTED
        );
    }

    let response = resend(&db_pool, &email).await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
}
