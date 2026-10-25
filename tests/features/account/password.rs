use axum::http::StatusCode;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, get_with_cookies, next_totp_code, post_json_with_cookies,
    recorded_event_types, response_json, session_cookie_header, set_cookie_headers, sign_in,
    sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

const NEW_PASSWORD: &str = "amber kettle hums beneath nine slow bridges";

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
async fn change_password_keeps_this_session_and_ends_the_others(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let other_sign_in = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;
    let other_session_cookies = session_cookie_header(&set_cookie_headers(&other_sign_in));

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/password",
        json!({ "currentPassword": STRONG_PASSWORD, "newPassword": NEW_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "password_changed" })
    );
    assert_eq!(
        session_status(&db_pool, &signed_in_person.session_cookies).await,
        StatusCode::OK
    );
    assert_eq!(
        session_status(&db_pool, &other_session_cookies).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, NEW_PASSWORD, "")
            .await
            .status(),
        StatusCode::OK
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PasswordChanged)
    );
}

#[sqlx::test]
async fn change_password_returns_422_for_a_wrong_current_password(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/password",
        json!({
            "currentPassword": "seven quiet docks drift past a violet lantern",
            "newPassword": NEW_PASSWORD,
        }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["error"],
        "your current password is incorrect"
    );
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::OK
    );
}

#[sqlx::test]
async fn change_password_applies_the_password_policy(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/account/password",
        json!({ "currentPassword": STRONG_PASSWORD, "newPassword": "short" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn change_password_returns_401_without_a_session(db_pool: PgPool) {
    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/account/password",
        json!({ "currentPassword": STRONG_PASSWORD, "newPassword": NEW_PASSWORD }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
