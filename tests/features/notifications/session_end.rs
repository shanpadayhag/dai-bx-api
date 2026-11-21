use axum::http::StatusCode;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::notifications::devices::device_body;
use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, get_with_cookies, next_totp_code,
    post_json_with_cookies, recorded_event_types, response_json, session_cookie_header,
    set_cookie_headers, sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

async fn sign_in_elsewhere(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    let verify_response = verify_code(
        db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;
    assert_eq!(verify_response.status(), StatusCode::OK);
    session_cookie_header(&set_cookie_headers(&verify_response))
}

async fn register_from(db_pool: &PgPool, session_cookies: &str, endpoint_suffix: &str) {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/push-devices",
        device_body(endpoint_suffix),
        session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn listed_device_count(db_pool: &PgPool, session_cookies: &str) -> usize {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/notifications",
        session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["devices"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default()
}

async fn post_from(
    db_pool: &PgPool,
    request_path: &str,
    request_body: Value,
    session_cookies: &str,
) -> StatusCode {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        request_path,
        request_body,
        session_cookies,
    )
    .await
    .status()
}

#[sqlx::test]
async fn signing_out_removes_only_the_devices_of_that_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_session_cookies = sign_in_elsewhere(&db_pool, &signed_in_person).await;
    register_from(&db_pool, &signed_in_person.session_cookies, "laptop").await;
    register_from(&db_pool, &other_session_cookies, "phone").await;

    let sign_out_status = post_from(
        &db_pool,
        "/auth/sign-out",
        json!({}),
        &other_session_cookies,
    )
    .await;

    assert_eq!(sign_out_status, StatusCode::OK);
    assert_eq!(
        listed_device_count(&db_pool, &signed_in_person.session_cookies).await,
        1
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PushDeviceRemoved)
    );
}

#[sqlx::test]
async fn ending_all_sessions_removes_every_device(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_session_cookies = sign_in_elsewhere(&db_pool, &signed_in_person).await;
    register_from(&db_pool, &signed_in_person.session_cookies, "laptop").await;
    register_from(&db_pool, &other_session_cookies, "phone").await;

    let end_all_status = post_from(
        &db_pool,
        "/account/sessions/end-all",
        json!({ "password": STRONG_PASSWORD }),
        &signed_in_person.session_cookies,
    )
    .await;
    let remaining_device_count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_devices")
        .fetch_one(&db_pool)
        .await
        .expect("device count should succeed");

    assert_eq!(end_all_status, StatusCode::OK);
    assert_eq!(remaining_device_count, 0);
}

#[sqlx::test]
async fn a_password_change_keeps_the_device_it_was_made_on(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_session_cookies = sign_in_elsewhere(&db_pool, &signed_in_person).await;
    register_from(&db_pool, &signed_in_person.session_cookies, "laptop").await;
    register_from(&db_pool, &other_session_cookies, "phone").await;

    let change_status = post_from(
        &db_pool,
        "/account/password",
        json!({
            "currentPassword": STRONG_PASSWORD,
            "newPassword": "amber kettle hums beneath nine slow bridges",
        }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(change_status, StatusCode::OK);
    assert_eq!(
        listed_device_count(&db_pool, &signed_in_person.session_cookies).await,
        1
    );
}
