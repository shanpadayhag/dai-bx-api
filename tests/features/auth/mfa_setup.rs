use axum::http::StatusCode;
use daibx_api::features::security_log::actions::list_security_events_for_subject::ListSecurityEventsForSubject;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, cookie_pair, current_totp_code, find_set_cookie, get_with_cookies,
    post_json_with_cookies, read_pending_totp_secret, response_json, set_cookie_headers,
    start_authenticator_setup,
};

fn wrong_code_for(correct_code: &str) -> String {
    correct_code
        .chars()
        .map(|code_digit| {
            let digit_value = code_digit.to_digit(10).expect("codes are digits");
            char::from_digit((digit_value + 5) % 10, 10).expect("a digit stays a digit")
        })
        .collect()
}

fn assert_host_cookie(set_cookie: &str, expected_max_age: &str) {
    for cookie_attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        expected_max_age,
    ] {
        assert!(
            set_cookie.contains(cookie_attribute),
            "missing {cookie_attribute} in {set_cookie}"
        );
    }
}

#[sqlx::test]
async fn get_mfa_setup_returns_the_otpauth_uri_and_the_key_for_manual_entry(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup",
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let setup_body = response_json(response).await;
    let otpauth_uri = setup_body["otpauthUri"].as_str().expect("a URI");
    let manual_entry_key = setup_body["manualEntryKey"].as_str().expect("a key");
    assert!(otpauth_uri.starts_with("otpauth://totp/DaiBX:"));
    assert!(otpauth_uri.contains("&issuer=DaiBX"));
    assert!(otpauth_uri.contains(&format!("secret={}&", manual_entry_key.replace(' ', ""))));
    assert_eq!(manual_entry_key.split(' ').count(), 8);
}

#[sqlx::test]
async fn get_mfa_setup_returns_401_without_a_challenge(db_pool: PgPool) {
    let response = get_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup",
        "__Host-daibx_challenge=qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn confirm_returns_200_sets_both_session_cookies_and_returns_ten_recovery_codes(
    db_pool: PgPool,
) {
    let setup_start = start_authenticator_setup(&db_pool).await;
    let totp_secret = read_pending_totp_secret(&db_pool, &setup_start.challenge_cookie).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/setup/confirm",
        json!({ "code": current_totp_code(&totp_secret) }),
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    let access_cookie = find_set_cookie(&set_cookies, "__Host-daibx_access");
    let refresh_cookie = find_set_cookie(&set_cookies, "__Host-daibx_refresh");
    assert_host_cookie(&access_cookie, "Max-Age=900");
    assert_host_cookie(&refresh_cookie, "Max-Age=604800");
    assert_host_cookie(
        &find_set_cookie(&set_cookies, "__Host-daibx_challenge"),
        "Max-Age=0",
    );
    let recovery_codes: Vec<String> = response_json(response).await["recoveryCodes"]
        .as_array()
        .expect("recovery codes should be a list")
        .iter()
        .map(|recovery_code| recovery_code.as_str().expect("a code").to_owned())
        .collect();
    assert_eq!(recovery_codes.len(), 10);
    for recovery_code in &recovery_codes {
        assert_eq!(recovery_code.len(), 14, "{recovery_code}");
        assert_eq!(recovery_code.matches('-').count(), 2, "{recovery_code}");
    }
    let mut distinct_codes = recovery_codes.clone();
    distinct_codes.sort();
    distinct_codes.dedup();
    assert_eq!(distinct_codes.len(), 10);

    let session_response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        &cookie_pair(&access_cookie),
    )
    .await;
    assert_eq!(session_response.status(), StatusCode::OK);

    let set_up_user = FindUserByEmail::new(&db_pool)
        .execute(&setup_start.email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    assert!(set_up_user.is_mfa_enabled);
    let recorded_event_types: Vec<SecurityEventType> = ListSecurityEventsForSubject::new()
        .execute(&db_pool, set_up_user.security_ref)
        .await
        .expect("listing should succeed")
        .into_iter()
        .map(|security_event| security_event.event_type)
        .collect();
    assert!(recorded_event_types.contains(&SecurityEventType::MfaSetUp));
}

#[sqlx::test]
async fn confirm_cannot_reuse_a_challenge_that_already_succeeded(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;
    let totp_secret = read_pending_totp_secret(&db_pool, &setup_start.challenge_cookie).await;
    let confirm_body = json!({ "code": current_totp_code(&totp_secret) });
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/setup/confirm",
        confirm_body.clone(),
        &setup_start.challenge_cookie,
    )
    .await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup/confirm",
        confirm_body,
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn confirm_counts_wrong_codes_and_refuses_the_sixth_try(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;
    let totp_secret = read_pending_totp_secret(&db_pool, &setup_start.challenge_cookie).await;

    for _ in 0..5 {
        let wrong_response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/auth/mfa/setup/confirm",
            json!({ "code": wrong_code_for(&current_totp_code(&totp_secret)) }),
            &setup_start.challenge_cookie,
        )
        .await;
        assert_eq!(wrong_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(wrong_response).await["error"],
            "that code didn't work, check your authenticator app"
        );
    }

    let sixth_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/setup/confirm",
        json!({ "code": current_totp_code(&totp_secret) }),
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(sixth_response.status(), StatusCode::UNAUTHORIZED);
    let set_up_user = FindUserByEmail::new(&db_pool)
        .execute(&setup_start.email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    assert!(!set_up_user.is_mfa_enabled);
}

#[sqlx::test]
async fn confirm_returns_401_for_an_expired_challenge(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;
    let totp_secret = read_pending_totp_secret(&db_pool, &setup_start.challenge_cookie).await;
    sqlx::query("UPDATE auth_challenges SET expires_at = now() - interval '1 minute'")
        .execute(&db_pool)
        .await
        .expect("expiring the challenge should succeed");

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup/confirm",
        json!({ "code": current_totp_code(&totp_secret) }),
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn confirm_returns_422_for_a_code_that_is_not_six_digits(db_pool: PgPool) {
    let setup_start = start_authenticator_setup(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/auth/mfa/setup/confirm",
        json!({ "code": "12ab" }),
        &setup_start.challenge_cookie,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
