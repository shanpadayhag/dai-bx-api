use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, cookie_pair, current_totp_code,
    find_set_cookie, get_with_cookies, next_totp_code, post_json_with_cookies,
    read_pending_totp_secret, recorded_event_types, response_json, session_cookie_header,
    set_cookie_headers, sign_in, sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

async fn start_replacement(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    entered_code: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/authenticator",
        json!({ "password": STRONG_PASSWORD, "code": entered_code }),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn confirm_replacement(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    challenge_cookie: &str,
    entered_code: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/authenticator/confirm",
        json!({ "code": entered_code }),
        &format!("{}; {challenge_cookie}", signed_in_person.session_cookies),
    )
    .await
}

async fn replacement_challenge(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    let start_response = start_replacement(
        db_pool,
        signed_in_person,
        &signed_in_person.recovery_codes[0],
    )
    .await;
    assert_eq!(start_response.status(), StatusCode::OK);
    let challenge_cookie = cookie_pair(&find_set_cookie(
        &set_cookie_headers(&start_response),
        "__Host-daibx_challenge",
    ));
    assert_eq!(
        response_json(start_response).await,
        json!({ "next": "set_up_authenticator" })
    );
    challenge_cookie
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
async fn replacing_with_a_recovery_code_swaps_the_secret_and_ends_other_sessions_and_devices(
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
    let other_session_cookies = session_cookie_header(&remembered_cookies);
    let device_cookie = cookie_pair(&find_set_cookie(&remembered_cookies, "__Host-daibx_device"));
    let replacement_cookie = replacement_challenge(&db_pool, &signed_in_person).await;
    let new_totp_secret = read_pending_totp_secret(&db_pool, &replacement_cookie).await;

    let response = confirm_replacement(
        &db_pool,
        &signed_in_person,
        &replacement_cookie,
        &current_totp_code(&new_totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    for cleared_cookie in ["__Host-daibx_challenge", "__Host-daibx_device"] {
        assert!(find_set_cookie(&set_cookies, cleared_cookie).contains("Max-Age=0"));
    }
    assert_eq!(
        response_json(response).await,
        json!({ "status": "authenticator_replaced" })
    );
    assert_eq!(
        session_status(&db_pool, &signed_in_person.session_cookies).await,
        StatusCode::OK
    );
    assert_eq!(
        session_status(&db_pool, &other_session_cookies).await,
        StatusCode::UNAUTHORIZED
    );
    let device_sign_in = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;
    assert_eq!(
        response_json(device_sign_in).await,
        json!({ "next": "enter_code" })
    );
    let old_secret_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let old_secret_response = verify_code(
        &db_pool,
        &old_secret_challenge,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;
    assert_eq!(
        old_secret_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let new_secret_response = verify_code(
        &db_pool,
        &old_secret_challenge,
        &next_totp_code(&new_totp_secret),
        false,
    )
    .await;
    assert_eq!(new_secret_response.status(), StatusCode::OK);
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::RecoveryCodeUsed)
    );
}

#[sqlx::test]
async fn confirming_the_replacement_with_a_wrong_code_keeps_the_old_authenticator(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let replacement_cookie = replacement_challenge(&db_pool, &signed_in_person).await;

    let response =
        confirm_replacement(&db_pool, &signed_in_person, &replacement_cookie, "000000").await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let old_secret_response = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;
    assert_eq!(old_secret_response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn confirming_without_a_replacement_step_returns_422(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = confirm_replacement(
        &db_pool,
        &signed_in_person,
        "__Host-daibx_challenge=qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw",
        "123456",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["error"],
        "this step has expired, start again"
    );
}

#[sqlx::test]
async fn starting_a_replacement_needs_the_password_and_a_code(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = start_replacement(&db_pool, &signed_in_person, "000000").await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn the_setup_confirm_endpoint_does_not_accept_a_replacement_step(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let replacement_cookie = replacement_challenge(&db_pool, &signed_in_person).await;
    let new_totp_secret = read_pending_totp_secret(&db_pool, &replacement_cookie).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup/confirm",
        json!({ "code": current_totp_code(&new_totp_secret) }),
        &replacement_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
