use axum::http::StatusCode;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, cookie_pair, find_set_cookie, get_with_cookies, next_totp_code,
    post_json_with_cookies, recorded_event_types, response_json, session_cookie_header,
    set_cookie_headers, sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

#[sqlx::test]
async fn verify_with_an_authenticator_code_signs_in(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;

    let response = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    let access_cookie = find_set_cookie(&set_cookies, "__Host-daibx_access");
    assert!(access_cookie.contains("Max-Age=900"));
    assert!(find_set_cookie(&set_cookies, "__Host-daibx_refresh").contains("Max-Age=604800"));
    assert!(find_set_cookie(&set_cookies, "__Host-daibx_challenge").contains("Max-Age=0"));
    assert!(
        !set_cookies
            .iter()
            .any(|set_cookie| set_cookie.starts_with("__Host-daibx_device=")),
        "the device is only remembered when ticked"
    );
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
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SignInSucceeded)
    );
}

#[sqlx::test]
async fn verify_refuses_an_authenticator_code_that_was_already_used(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let used_code = next_totp_code(&signed_in_person.totp_secret);
    let first_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let first_response = verify_code(&db_pool, &first_challenge, &used_code, false).await;
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let reused_response = verify_code(&db_pool, &second_challenge, &used_code, false).await;

    assert_eq!(reused_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn verify_with_a_recovery_code_reports_how_many_remain_and_refuses_it_after(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let typed_recovery_code = signed_in_person.recovery_codes[0]
        .replace('-', "")
        .to_lowercase();
    let first_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;

    let first_response = verify_code(&db_pool, &first_challenge, &typed_recovery_code, false).await;

    assert_eq!(first_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(first_response).await,
        json!({ "next": "signed_in", "remainingRecoveryCodes": 9 })
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::RecoveryCodeUsed)
    );

    let second_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let reused_response = verify_code(
        &db_pool,
        &second_challenge,
        &signed_in_person.recovery_codes[0],
        false,
    )
    .await;
    assert_eq!(reused_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let another_response = verify_code(
        &db_pool,
        &second_challenge,
        &signed_in_person.recovery_codes[1],
        false,
    )
    .await;
    assert_eq!(
        response_json(another_response).await["remainingRecoveryCodes"],
        8
    );
}

#[sqlx::test]
async fn verify_remembers_the_device_for_30_days_when_ticked(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;

    let response = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let device_cookie = find_set_cookie(&set_cookie_headers(&response), "__Host-daibx_device");
    for cookie_attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=2592000",
    ] {
        assert!(
            device_cookie.contains(cookie_attribute),
            "missing {cookie_attribute} in {device_cookie}"
        );
    }
}

#[sqlx::test]
async fn signing_in_again_ends_the_session_this_browser_already_had(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/verify",
        json!({ "code": next_totp_code(&signed_in_person.totp_secret), "rememberDevice": false }),
        &format!("{}; {challenge_cookie}", signed_in_person.session_cookies),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let new_session_cookies = session_cookie_header(&set_cookie_headers(&response));
    for (session_cookies, expected_status) in [
        (&signed_in_person.session_cookies, StatusCode::UNAUTHORIZED),
        (&new_session_cookies, StatusCode::OK),
    ] {
        let session_response = get_with_cookies(
            build_test_app(db_pool.clone()),
            "/auth/session",
            session_cookies,
        )
        .await;
        assert_eq!(session_response.status(), expected_status);
    }
}

#[sqlx::test]
async fn verify_returns_401_without_a_sign_in_challenge(db_pool: PgPool) {
    let response = verify_code(
        &db_pool,
        "__Host-daibx_challenge=qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw",
        "123456",
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn verify_throttles_wrong_codes_across_challenges(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let first_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    for _ in 0..5 {
        let wrong_response = verify_code(&db_pool, &first_challenge, "000000", false).await;
        assert_eq!(wrong_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let sixth_on_first = verify_code(&db_pool, &first_challenge, "000000", false).await;
    assert_eq!(sixth_on_first.status(), StatusCode::UNAUTHORIZED);

    let second_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let wrong_on_second = verify_code(&db_pool, &second_challenge, "000000", false).await;
    assert_eq!(wrong_on_second.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let blocked_response = verify_code(
        &db_pool,
        &second_challenge,
        &next_totp_code(&signed_in_person.totp_secret),
        false,
    )
    .await;

    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
}
