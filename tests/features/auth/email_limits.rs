use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, count_emails_after_a_pause, count_sent_account_emails,
    create_invite_code, post_json, response_json, send, sign_up_with_authenticator, unique_email,
};

const SECONDS_IN_23_HOURS: u64 = 23 * 60 * 60;

async fn sign_up(db_pool: &PgPool, email: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        json!({
            "inviteCode": create_invite_code(db_pool).await,
            "name": "Shan Padayhag",
            "email": email,
            "password": STRONG_PASSWORD,
        }),
    )
    .await
}

async fn request_reset(db_pool: &PgPool, email: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/password-reset",
        json!({ "email": email }),
    )
    .await
}

async fn resend_verification(db_pool: &PgPool, email: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email/resend",
        json!({ "email": email }),
    )
    .await
}

async fn request_reset_from(db_pool: &PgPool, email: &str, client_ip: &str) -> Response<Body> {
    let request = Request::post("/auth/password-reset")
        .header("sec-fetch-site", "same-origin")
        .header("content-type", "application/json")
        .header("x-vercel-forwarded-for", client_ip)
        .body(Body::from(json!({ "email": email }).to_string()))
        .expect("test request should build");
    send(build_test_app(db_pool.clone()), request).await
}

fn retry_after_seconds(response: &Response<Body>) -> u64 {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .expect("a refused request should say when to retry")
        .to_str()
        .expect("Retry-After should be text")
        .parse()
        .expect("Retry-After should be a number of seconds")
}

#[sqlx::test]
async fn sign_up_returns_429_on_the_fourth_request_for_one_address_in_an_hour(db_pool: PgPool) {
    let email = unique_email();
    for _ in 0..3 {
        assert_eq!(
            sign_up(&db_pool, &email).await.status(),
            StatusCode::ACCEPTED
        );
    }

    let response = sign_up(&db_pool, &email).await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(retry_after_seconds(&response) > 0);
}

#[sqlx::test]
async fn sign_up_checks_its_limit_before_the_invite(db_pool: PgPool) {
    let email = unique_email();
    for _ in 0..3 {
        resend_verification(&db_pool, &email).await;
    }

    let response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        json!({
            "inviteCode": "not-a-real-invite",
            "name": "Shan Padayhag",
            "email": email,
            "password": STRONG_PASSWORD,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn sign_up_reset_and_resend_share_ten_requests_an_hour_per_ip_address(db_pool: PgPool) {
    for request_number in 0..10 {
        let email = unique_email();
        let response = match request_number % 3 {
            0 => sign_up(&db_pool, &email).await,
            1 => request_reset(&db_pool, &email).await,
            _ => resend_verification(&db_pool, &email).await,
        };
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    let refused_response = request_reset(&db_pool, &unique_email()).await;
    let other_address_response = request_reset_from(&db_pool, &unique_email(), "203.0.113.9").await;

    assert_eq!(refused_response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(retry_after_seconds(&refused_response) > 0);
    assert_eq!(other_address_response.status(), StatusCode::ACCEPTED);
}

#[sqlx::test]
async fn a_full_daily_cap_refuses_known_and_unknown_addresses_alike(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    count_sent_account_emails(&db_pool, 99).await;

    let known_reset = request_reset(&db_pool, &signed_in_person.email).await;
    let unknown_reset = request_reset(&db_pool, &unique_email()).await;
    let known_resend = resend_verification(&db_pool, &signed_in_person.email).await;
    let unknown_resend = resend_verification(&db_pool, &unique_email()).await;

    for response in [&known_reset, &unknown_reset, &known_resend, &unknown_resend] {
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(retry_after_seconds(response) > SECONDS_IN_23_HOURS);
    }
    let known_reset_body = response_json(known_reset).await;
    assert_eq!(response_json(unknown_reset).await, known_reset_body);
    assert_eq!(response_json(known_resend).await, known_reset_body);
    assert_eq!(response_json(unknown_resend).await, known_reset_body);
    assert_eq!(count_emails_after_a_pause(&signed_in_person.email).await, 1);
}

#[sqlx::test]
async fn a_full_daily_cap_refuses_sign_up_without_creating_an_account(db_pool: PgPool) {
    count_sent_account_emails(&db_pool, 100).await;
    let email = unique_email();

    let response = sign_up(&db_pool, &email).await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        FindUserByEmail::new(&db_pool)
            .execute(&email)
            .await
            .expect("lookup should succeed"),
        None
    );
}

#[sqlx::test]
async fn made_up_addresses_do_not_use_up_the_daily_cap(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    count_sent_account_emails(&db_pool, 98).await;
    for _ in 0..3 {
        assert_eq!(
            request_reset(&db_pool, &unique_email()).await.status(),
            StatusCode::ACCEPTED
        );
    }

    let last_real_send = request_reset(&db_pool, &signed_in_person.email).await;
    let first_refused = request_reset(&db_pool, &unique_email()).await;

    assert_eq!(last_real_send.status(), StatusCode::ACCEPTED);
    assert_eq!(first_refused.status(), StatusCode::TOO_MANY_REQUESTS);
}
