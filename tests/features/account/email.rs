use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, count_emails_after_a_pause, get_with_cookies,
    latest_link_token, next_totp_code, post_json, post_json_with_cookies, recorded_event_types,
    response_json, sign_in, sign_up_with_authenticator, unique_email,
};

async fn request_email_change(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    password: &str,
    entered_code: &str,
    new_email: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/email",
        json!({ "password": password, "code": entered_code, "newEmail": new_email }),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn confirm_email_change(db_pool: &PgPool, link_token: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/account/email/confirm",
        json!({ "token": link_token }),
    )
    .await
}

async fn account_email(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    let account_response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;
    response_json(account_response).await["email"]
        .as_str()
        .expect("the account should have an email")
        .to_owned()
}

#[sqlx::test]
async fn change_email_sends_a_link_to_the_new_address_and_switches_only_once_confirmed(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let new_email = unique_email();

    let response = request_email_change(
        &db_pool,
        &signed_in_person,
        STRONG_PASSWORD,
        &next_totp_code(&signed_in_person.totp_secret),
        &new_email,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "confirmation_email_sent" })
    );
    let link_token = latest_link_token(&new_email, 1).await;
    assert_eq!(
        account_email(&db_pool, &signed_in_person).await,
        signed_in_person.email
    );

    let confirm_response = confirm_email_change(&db_pool, &link_token).await;

    assert_eq!(confirm_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(confirm_response).await,
        json!({ "status": "email_changed" })
    );
    assert_eq!(account_email(&db_pool, &signed_in_person).await, new_email);
    assert_eq!(
        sign_in(&db_pool, &new_email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        recorded_event_types(&db_pool, &new_email)
            .await
            .contains(&SecurityEventType::EmailChanged)
    );
    assert_eq!(
        confirm_email_change(&db_pool, &link_token).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test]
async fn change_email_accepts_a_recovery_code_instead_of_an_authenticator_code(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let new_email = unique_email();

    let response = request_email_change(
        &db_pool,
        &signed_in_person,
        STRONG_PASSWORD,
        &signed_in_person.recovery_codes[0],
        &new_email,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    latest_link_token(&new_email, 1).await;
}

#[sqlx::test]
async fn change_email_refuses_a_wrong_password_or_code_and_sends_nothing(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let new_email = unique_email();

    let wrong_password_response = request_email_change(
        &db_pool,
        &signed_in_person,
        "seven quiet docks drift past a violet lantern",
        &next_totp_code(&signed_in_person.totp_secret),
        &new_email,
    )
    .await;
    let wrong_code_response = request_email_change(
        &db_pool,
        &signed_in_person,
        STRONG_PASSWORD,
        "000000",
        &new_email,
    )
    .await;

    assert_eq!(
        wrong_password_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(wrong_password_response).await["error"],
        "your password is incorrect"
    );
    assert_eq!(
        wrong_code_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(count_emails_after_a_pause(&new_email).await, 0);
}

#[sqlx::test]
async fn change_email_answers_a_taken_address_the_same_way_and_sends_nothing(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;

    let response = request_email_change(
        &db_pool,
        &signed_in_person,
        STRONG_PASSWORD,
        &next_totp_code(&signed_in_person.totp_secret),
        &other_person.email.to_uppercase(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "confirmation_email_sent" })
    );
    assert_eq!(count_emails_after_a_pause(&other_person.email).await, 1);
}

#[sqlx::test]
async fn confirm_email_change_returns_422_for_an_unknown_link(db_pool: PgPool) {
    let response =
        confirm_email_change(&db_pool, "qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw").await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
