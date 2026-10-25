use axum::body::Body;
use axum::http::{Response, StatusCode, header};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, cookie_pair, count_emails_after_a_pause,
    find_set_cookie, get_with_cookies, latest_link_token, next_totp_code, post_json,
    recorded_event_types, response_json, session_cookie_header, set_cookie_headers, sign_in,
    sign_in_to_code_step, sign_up_with_authenticator, unique_email, verify_code,
};

const NEW_PASSWORD: &str = "amber kettle hums beneath nine slow bridges";

async fn request_reset(db_pool: &PgPool, email: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/password-reset",
        json!({ "email": email }),
    )
    .await
}

async fn complete_reset(db_pool: &PgPool, link_token: &str, password: &str) -> Response<Body> {
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/password-reset/complete",
        json!({ "token": link_token, "password": password }),
    )
    .await
}

async fn reset_link_token(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    let request_response = request_reset(db_pool, &signed_in_person.email).await;
    assert_eq!(request_response.status(), StatusCode::ACCEPTED);
    latest_link_token(&signed_in_person.email, 2).await
}

async fn session_status(db_pool: &PgPool, cookie_header: &str) -> StatusCode {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        cookie_header,
    )
    .await
    .status()
}

#[sqlx::test]
async fn reset_request_answers_known_and_unknown_emails_the_same_and_only_emails_the_known(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let unknown_email = unique_email();

    let known_response = request_reset(&db_pool, &signed_in_person.email).await;
    let unknown_response = request_reset(&db_pool, &unknown_email).await;

    assert_eq!(known_response.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown_response.status(), StatusCode::ACCEPTED);
    let known_body = response_json(known_response).await;
    assert_eq!(known_body, json!({ "status": "reset_email_sent" }));
    assert_eq!(response_json(unknown_response).await, known_body);
    latest_link_token(&signed_in_person.email, 2).await;
    assert_eq!(count_emails_after_a_pause(&unknown_email).await, 0);
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PasswordResetRequested)
    );
}

#[sqlx::test]
async fn reset_request_returns_429_after_three_requests_an_hour(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for _ in 0..3 {
        assert_eq!(
            request_reset(&db_pool, &signed_in_person.email)
                .await
                .status(),
            StatusCode::ACCEPTED
        );
    }

    let fourth_response = request_reset(&db_pool, &signed_in_person.email).await;

    assert_eq!(fourth_response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(fourth_response.headers().contains_key(header::RETRY_AFTER));
}

#[sqlx::test]
async fn reset_complete_revokes_every_session_and_device_and_still_asks_for_a_code(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let remembered_response = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;
    let remembered_cookies = set_cookie_headers(&remembered_response);
    let remembered_session_cookies = session_cookie_header(&remembered_cookies);
    let device_cookie = cookie_pair(&find_set_cookie(&remembered_cookies, "__Host-daibx_device"));
    let link_token = reset_link_token(&db_pool, &signed_in_person).await;

    let response = complete_reset(&db_pool, &link_token, NEW_PASSWORD).await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    let reset_challenge_cookie =
        cookie_pair(&find_set_cookie(&set_cookies, "__Host-daibx_challenge"));
    for cleared_cookie in [
        "__Host-daibx_access",
        "__Host-daibx_refresh",
        "__Host-daibx_device",
    ] {
        assert!(find_set_cookie(&set_cookies, cleared_cookie).contains("Max-Age=0"));
    }
    assert_eq!(
        response_json(response).await,
        json!({ "next": "enter_code" })
    );
    for old_session_cookies in [
        &signed_in_person.session_cookies,
        &remembered_session_cookies,
    ] {
        assert_eq!(
            session_status(&db_pool, old_session_cookies).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let device_sign_in = sign_in(
        &db_pool,
        &signed_in_person.email,
        NEW_PASSWORD,
        &device_cookie,
    )
    .await;
    assert_eq!(
        response_json(device_sign_in).await,
        json!({ "next": "enter_code" })
    );
    let code_response = verify_code(
        &db_pool,
        &reset_challenge_cookie,
        &signed_in_person.recovery_codes[0],
        false,
    )
    .await;
    assert_eq!(code_response.status(), StatusCode::OK);
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PasswordResetCompleted)
    );
}

#[sqlx::test]
async fn reset_complete_refuses_a_link_that_was_already_used(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let link_token = reset_link_token(&db_pool, &signed_in_person).await;
    complete_reset(&db_pool, &link_token, NEW_PASSWORD).await;

    let reused_response = complete_reset(
        &db_pool,
        &link_token,
        "copper lanterns sway over quiet harbour stones",
    )
    .await;

    assert_eq!(reused_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(reused_response).await["error"],
        "this link is invalid or has expired"
    );
}

#[sqlx::test]
async fn reset_complete_refuses_a_link_older_than_30_minutes(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let link_token = reset_link_token(&db_pool, &signed_in_person).await;
    sqlx::query(
        "UPDATE email_tokens SET expires_at = now() - interval '1 minute' WHERE purpose = 'password_reset'",
    )
    .execute(&db_pool)
    .await
    .expect("expiring the link should succeed");

    let response = complete_reset(&db_pool, &link_token, NEW_PASSWORD).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        session_status(&db_pool, &signed_in_person.session_cookies).await,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn reset_complete_keeps_the_link_usable_when_the_new_password_is_refused(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let link_token = reset_link_token(&db_pool, &signed_in_person).await;

    let weak_response = complete_reset(&db_pool, &link_token, "short").await;
    let strong_response = complete_reset(&db_pool, &link_token, NEW_PASSWORD).await;

    assert_eq!(weak_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(strong_response.status(), StatusCode::OK);
}
