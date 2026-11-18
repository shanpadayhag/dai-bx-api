use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::core::security::opaque_token::hash_opaque_token;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, cookie_pair, cookie_value,
    count_known_browsers, current_totp_code, find_set_cookie, next_totp_code,
    post_json_with_cookies, read_pending_totp_secret, response_json, set_cookie_headers, sign_in,
    sign_in_to_code_step, sign_up_with_authenticator, start_authenticator_setup, verify_code,
};

const BROWSER_COOKIE_NAME: &str = "__Host-daibx_browser";
const SESSION_COOKIE_NAME: &str = "__Host-daibx_access";

fn browser_cookie_in(response: &Response<Body>) -> String {
    cookie_pair(&find_set_cookie(
        &set_cookie_headers(response),
        BROWSER_COOKIE_NAME,
    ))
}

fn sets_cookie(response: &Response<Body>, cookie_name: &str) -> bool {
    set_cookie_headers(response)
        .iter()
        .any(|set_cookie| set_cookie.starts_with(&format!("{cookie_name}=")))
}

async fn known_browser_hashes(db_pool: &PgPool, email: &str) -> Vec<Vec<u8>> {
    sqlx::query_scalar(
        "SELECT known_browsers.token_hash
         FROM known_browsers
         JOIN users ON users.id = known_browsers.user_id
         WHERE users.email::text = $1
         ORDER BY known_browsers.id",
    )
    .bind(email)
    .fetch_all(db_pool)
    .await
    .expect("listing known browsers should succeed")
}

async fn age_known_browsers(db_pool: &PgPool, last_seen_ago: &str, expires_in: &str) {
    sqlx::query(
        "UPDATE known_browsers
         SET last_seen_at = now() - $1::interval, expires_at = now() + $2::interval",
    )
    .bind(last_seen_ago)
    .bind(expires_in)
    .execute(db_pool)
    .await
    .expect("ageing known browsers should succeed");
}

async fn known_browser_was_just_extended(db_pool: &PgPool) -> bool {
    sqlx::query_scalar(
        "SELECT expires_at > now() + interval '364 days'
                AND last_seen_at > now() - interval '1 minute'
         FROM known_browsers",
    )
    .fetch_one(db_pool)
    .await
    .expect("reading the known browser should succeed")
}

async fn sign_in_with_code_from_browser(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    browser_cookie: &str,
    entered_code: &str,
) -> Response<Body> {
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    verify_code(
        db_pool,
        &format!("{challenge_cookie}; {browser_cookie}"),
        entered_code,
        false,
    )
    .await
}

#[sqlx::test]
async fn first_authenticator_setup_makes_the_browser_known_with_a_hashed_token(db_pool: PgPool) {
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
    let browser_set_cookie = find_set_cookie(&set_cookie_headers(&response), BROWSER_COOKIE_NAME);
    let browser_cookie = cookie_pair(&browser_set_cookie);
    let browser_token = cookie_value(&browser_cookie, BROWSER_COOKIE_NAME);
    assert_eq!(browser_token.len(), 43, "256 random bits in base64url");
    assert_eq!(
        browser_set_cookie,
        format!(
            "{BROWSER_COOKIE_NAME}={browser_token}; Path=/; Max-Age=31536000; HttpOnly; Secure; SameSite=Lax"
        )
    );
    assert_eq!(
        known_browser_hashes(&db_pool, &setup_start.email).await,
        vec![hash_opaque_token(browser_token).to_vec()],
        "only the token's hash is stored"
    );
}

#[sqlx::test]
async fn a_code_sign_in_makes_a_new_browser_known_without_remembering_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        "",
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(!sets_cookie(&response, "__Host-daibx_device"));
    let browser_cookie = browser_cookie_in(&response);
    assert_ne!(browser_cookie, signed_in_person.browser_cookie);
    assert!(
        known_browser_hashes(&db_pool, &signed_in_person.email)
            .await
            .contains(
                &hash_opaque_token(cookie_value(&browser_cookie, BROWSER_COOKIE_NAME)).to_vec()
            )
    );
    assert_eq!(count_known_browsers(&db_pool).await, 2);
}

#[sqlx::test]
async fn a_code_sign_in_keeps_a_known_browser_for_another_year(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    age_known_browsers(&db_pool, "200 days", "165 days").await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        &signed_in_person.browser_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let browser_set_cookie = find_set_cookie(&set_cookie_headers(&response), BROWSER_COOKIE_NAME);
    assert_eq!(
        cookie_pair(&browser_set_cookie),
        signed_in_person.browser_cookie
    );
    assert!(browser_set_cookie.contains("Max-Age=31536000"));
    assert_eq!(count_known_browsers(&db_pool).await, 1);
    assert!(known_browser_was_just_extended(&db_pool).await);
}

#[sqlx::test]
async fn an_expired_known_browser_is_replaced(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    age_known_browsers(&db_pool, "2 years", "-1 minute").await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        &signed_in_person.browser_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ne!(
        browser_cookie_in(&response),
        signed_in_person.browser_cookie
    );
}

#[sqlx::test]
async fn another_accounts_browser_token_is_not_accepted(db_pool: PgPool) {
    let first_person = sign_up_with_authenticator(&db_pool).await;
    let second_person = sign_up_with_authenticator(&db_pool).await;
    let first_person_hashes = known_browser_hashes(&db_pool, &first_person.email).await;

    let response = sign_in_with_code_from_browser(
        &db_pool,
        &second_person,
        &first_person.browser_cookie,
        &next_totp_code(&second_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ne!(browser_cookie_in(&response), first_person.browser_cookie);
    assert_eq!(
        known_browser_hashes(&db_pool, &first_person.email).await,
        first_person_hashes
    );
    assert_eq!(
        known_browser_hashes(&db_pool, &second_person.email)
            .await
            .len(),
        2
    );
}

#[sqlx::test]
async fn a_remembered_device_sign_in_makes_the_browser_known(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let remembered_response = verify_code(
        &db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;
    let device_cookie = cookie_pair(&find_set_cookie(
        &set_cookie_headers(&remembered_response),
        "__Host-daibx_device",
    ));
    sqlx::query("DELETE FROM known_browsers")
        .execute(&db_pool)
        .await
        .expect("clearing known browsers should succeed");

    let new_browser_response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &device_cookie,
    )
    .await;

    assert_eq!(new_browser_response.status(), StatusCode::OK);
    let browser_cookie = browser_cookie_in(&new_browser_response);
    assert_eq!(
        response_json(new_browser_response).await,
        json!({ "next": "signed_in" })
    );
    assert_eq!(count_known_browsers(&db_pool).await, 1);
    age_known_browsers(&db_pool, "200 days", "165 days").await;

    let known_browser_response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &format!("{device_cookie}; {browser_cookie}"),
    )
    .await;

    assert_eq!(browser_cookie_in(&known_browser_response), browser_cookie);
    assert_eq!(count_known_browsers(&db_pool).await, 1);
    assert!(known_browser_was_just_extended(&db_pool).await);
}

#[sqlx::test]
async fn a_known_browser_still_needs_the_password_and_the_code(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    age_known_browsers(&db_pool, "200 days", "165 days").await;

    let wrong_password_response = sign_in(
        &db_pool,
        &signed_in_person.email,
        "not the right password at all",
        &signed_in_person.browser_cookie,
    )
    .await;

    assert_eq!(wrong_password_response.status(), StatusCode::UNAUTHORIZED);
    assert!(!sets_cookie(&wrong_password_response, SESSION_COOKIE_NAME));

    let right_password_response = sign_in(
        &db_pool,
        &signed_in_person.email,
        STRONG_PASSWORD,
        &signed_in_person.browser_cookie,
    )
    .await;

    assert!(!sets_cookie(&right_password_response, SESSION_COOKIE_NAME));
    assert!(!sets_cookie(&right_password_response, BROWSER_COOKIE_NAME));
    assert_eq!(
        response_json(right_password_response).await,
        json!({ "next": "enter_code" })
    );

    let wrong_code_response = sign_in_with_code_from_browser(
        &db_pool,
        &signed_in_person,
        &signed_in_person.browser_cookie,
        "000000",
    )
    .await;

    assert_eq!(
        wrong_code_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(!sets_cookie(&wrong_code_response, SESSION_COOKIE_NAME));
    assert!(!sets_cookie(&wrong_code_response, BROWSER_COOKIE_NAME));
    assert!(
        !known_browser_was_just_extended(&db_pool).await,
        "a refused sign-in leaves the known browser alone"
    );
}

#[sqlx::test]
async fn signing_out_keeps_the_browser_known(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/sign-out",
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(!sets_cookie(&response, BROWSER_COOKIE_NAME));
    assert_eq!(count_known_browsers(&db_pool).await, 1);
}
