use argon2::password_hash::PasswordHasher;
use argon2::{Algorithm, Argon2, Params, Version};
use axum::http::{StatusCode, header};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, build_test_app, cookie_pair, create_invite_code, find_set_cookie,
    get_with_cookies, next_totp_code, post_json, recorded_event_types, response_json,
    set_cookie_headers, sign_in, sign_in_to_code_step, sign_up_with_authenticator,
    start_authenticator_setup, unique_email, verify_code,
};

const WRONG_PASSWORD: &str = "seven quiet docks drift past a violet lantern";

async fn remember_this_device(db_pool: &PgPool, email: &str, totp_secret: &[u8]) -> String {
    let challenge_cookie = sign_in_to_code_step(db_pool, email).await;
    let verify_response = verify_code(
        db_pool,
        &challenge_cookie,
        &next_totp_code(totp_secret),
        true,
    )
    .await;
    assert_eq!(verify_response.status(), StatusCode::OK);
    cookie_pair(&find_set_cookie(
        &set_cookie_headers(&verify_response),
        "__Host-daibx_device",
    ))
}

#[sqlx::test]
async fn sign_in_asks_for_a_code_after_a_correct_password(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    assert!(find_set_cookie(&set_cookies, "__Host-daibx_challenge").contains("Max-Age=300"));
    assert!(
        !set_cookies
            .iter()
            .any(|set_cookie| set_cookie.starts_with("__Host-daibx_access=")),
        "no session before the code step"
    );
    assert_eq!(
        response_json(response).await,
        json!({ "next": "enter_code" })
    );
}

#[sqlx::test]
async fn sign_in_answers_a_wrong_password_and_an_unknown_email_identically(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let wrong_password_response =
        sign_in(&db_pool, &signed_in_person.email, WRONG_PASSWORD, "").await;
    let unknown_email_response = sign_in(&db_pool, &unique_email(), STRONG_PASSWORD, "").await;

    assert_eq!(wrong_password_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_email_response.status(), StatusCode::UNAUTHORIZED);
    let wrong_password_body = response_json(wrong_password_response).await;
    assert_eq!(
        wrong_password_body,
        json!({ "error": "the email or password is incorrect" })
    );
    assert_eq!(
        response_json(unknown_email_response).await,
        wrong_password_body
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SignInFailed)
    );
}

#[sqlx::test]
async fn sign_in_tells_an_unverified_account_to_verify_only_after_a_correct_password(
    db_pool: PgPool,
) {
    let email = unique_email();
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        json!({
            "inviteCode": create_invite_code(&db_pool).await,
            "name": "Shan Padayhag",
            "email": email,
            "password": STRONG_PASSWORD,
        }),
    )
    .await;

    let wrong_password_response = sign_in(&db_pool, &email, WRONG_PASSWORD, "").await;
    let correct_password_response = sign_in(&db_pool, &email, STRONG_PASSWORD, "").await;

    assert_eq!(wrong_password_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(correct_password_response.status(), StatusCode::OK);
    assert!(set_cookie_headers(&correct_password_response).is_empty());
    assert_eq!(
        response_json(correct_password_response).await,
        json!({ "next": "verify_email" })
    );
}

#[sqlx::test]
async fn sign_in_sends_an_account_that_never_finished_mfa_to_authenticator_setup(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;

    let response = sign_in(&db_pool, &setup_start.email, STRONG_PASSWORD, "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let challenge_cookie = cookie_pair(&find_set_cookie(
        &set_cookie_headers(&response),
        "__Host-daibx_challenge",
    ));
    assert_eq!(
        response_json(response).await,
        json!({ "next": "set_up_authenticator" })
    );
    let setup_response = get_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup",
        &challenge_cookie,
    )
    .await;
    assert_eq!(setup_response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn sign_in_skips_the_code_on_a_remembered_device(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_cookie = remember_this_device(
        &db_pool,
        &signed_in_person.email,
        &signed_in_person.totp_secret,
    )
    .await;

    let response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    let access_cookie = find_set_cookie(&set_cookies, "__Host-daibx_access");
    find_set_cookie(&set_cookies, "__Host-daibx_refresh");
    assert_eq!(
        response_json(response).await,
        json!({ "next": "signed_in" })
    );
    let session_response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        &cookie_pair(&access_cookie),
    )
    .await;
    assert_eq!(session_response.status(), StatusCode::OK);
    let sign_in_count = recorded_event_types(&db_pool, &signed_in_person.email)
        .await
        .into_iter()
        .filter(|event_type| *event_type == SecurityEventType::SignInSucceeded)
        .count();
    assert_eq!(sign_in_count, 2);
}

#[sqlx::test]
async fn sign_in_asks_for_a_code_when_the_device_is_past_30_days(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_cookie = remember_this_device(
        &db_pool,
        &signed_in_person.email,
        &signed_in_person.totp_secret,
    )
    .await;
    sqlx::query("UPDATE remembered_devices SET expires_at = now() - interval '1 minute'")
        .execute(&db_pool)
        .await
        .expect("expiring the device should succeed");

    let response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "next": "enter_code" })
    );
}

#[sqlx::test]
async fn sign_in_asks_for_a_code_when_the_device_belongs_to_another_account(db_pool: PgPool) {
    let device_owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let device_cookie =
        remember_this_device(&db_pool, &device_owner.email, &device_owner.totp_secret).await;

    let response = sign_in(
        &db_pool,
        &other_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "next": "enter_code" })
    );
}

#[sqlx::test]
async fn sign_in_returns_429_with_retry_after_once_failures_repeat(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for _ in 0..6 {
        let failed_response = sign_in(&db_pool, &signed_in_person.email, WRONG_PASSWORD, "").await;
        assert_eq!(failed_response.status(), StatusCode::UNAUTHORIZED);
    }
    let blocked_response = sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "").await;

    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after_seconds: u64 = blocked_response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|retry_after_text| retry_after_text.parse().ok())
        .expect("a 429 should say when to retry");
    assert!((1..=60).contains(&retry_after_seconds));
}

#[sqlx::test]
async fn sign_in_rehashes_a_password_stored_with_outdated_settings(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let outdated_params = Params::new(8192, 1, 1, None).expect("valid outdated settings");
    let outdated_hash = Argon2::new(Algorithm::Argon2id, Version::V0x13, outdated_params)
        .hash_password(STRONG_PASSWORD.as_bytes())
        .expect("hashing should succeed")
        .to_string();
    sqlx::query("UPDATE users SET password_hash = $1")
        .bind(&outdated_hash)
        .execute(&db_pool)
        .await
        .expect("storing the outdated hash should succeed");

    let response = sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let rehashed_user = FindUserByEmail::new(&db_pool)
        .execute(&signed_in_person.email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    assert_ne!(rehashed_user.password_hash, outdated_hash);
    assert!(
        rehashed_user.password_hash.contains("m=19456,t=2,p=1"),
        "{}",
        rehashed_user.password_hash
    );
}
