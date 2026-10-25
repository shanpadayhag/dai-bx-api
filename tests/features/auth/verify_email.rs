use axum::http::{StatusCode, header};
use daibx_api::features::security_log::actions::list_security_events_for_subject::ListSecurityEventsForSubject;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, create_invite_code, latest_link_token, post_json,
    response_json, unique_email,
};

async fn sign_up_and_read_link_token(db_pool: &PgPool, email: &str) -> String {
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
    latest_link_token(email, 1).await
}

#[sqlx::test]
async fn verify_email_returns_200_marks_the_email_verified_and_starts_authenticator_setup(
    db_pool: PgPool,
) {
    let email = unique_email();
    let link_token = sign_up_and_read_link_token(&db_pool, &email).await;

    let response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let challenge_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|header_value| header_value.to_str().ok())
        .expect("a challenge cookie should be set")
        .to_owned();
    assert!(challenge_cookie.starts_with("__Host-daibx_challenge="));
    for cookie_attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=300",
    ] {
        assert!(
            challenge_cookie.contains(cookie_attribute),
            "missing {cookie_attribute} in {challenge_cookie}"
        );
    }
    assert_eq!(
        response_json(response).await["next"],
        "set_up_authenticator"
    );
    let verified_user = FindUserByEmail::new(&db_pool)
        .execute(&email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    assert!(verified_user.is_email_verified);
    let recorded_event_types: Vec<SecurityEventType> = ListSecurityEventsForSubject::new()
        .execute(&db_pool, verified_user.security_ref)
        .await
        .expect("listing should succeed")
        .into_iter()
        .map(|security_event| security_event.event_type)
        .collect();
    assert!(recorded_event_types.contains(&SecurityEventType::EmailVerified));
}

#[sqlx::test]
async fn verify_email_returns_422_for_a_link_that_was_already_used(db_pool: PgPool) {
    let link_token = sign_up_and_read_link_token(&db_pool, &unique_email()).await;
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;

    let response = post_json(
        build_test_app(db_pool),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn verify_email_returns_422_for_a_link_older_than_24_hours(db_pool: PgPool) {
    let link_token = sign_up_and_read_link_token(&db_pool, &unique_email()).await;
    sqlx::query("UPDATE email_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&db_pool)
        .await
        .expect("expiring the link should succeed");

    let response = post_json(
        build_test_app(db_pool),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn verify_email_returns_422_for_an_unknown_link(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool),
        "/auth/verify-email",
        json!({ "token": "qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["error"],
        "this link is invalid or has expired"
    );
}
