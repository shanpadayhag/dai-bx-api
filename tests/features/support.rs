use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use daibx_api::app;
use daibx_api::core::config::app_config::{AllowedOrigin, AppConfig};
use daibx_api::core::config::smtp_settings::{SmtpSecurity, SmtpSettings};
use daibx_api::core::http_client::build_http_client;
use daibx_api::core::mail_transport::build_mail_transport;
use daibx_api::core::security::totp::{totp_code_at_step, totp_step_at};
use daibx_api::core::security::totp_secret_cipher::TotpKeyring;
use daibx_api::core::state::AppState;
use daibx_api::features::invites::actions::create_invite::CreateInvite;
use daibx_api::features::security_log::actions::list_security_events_for_subject::ListSecurityEventsForSubject;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use data_encoding::BASE32_NOPAD;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const UNREACHABLE_HIBP_BASE_URL: &str = "http://127.0.0.1:9";
const MAILPIT_API_URL: &str = "http://127.0.0.1:8025";
const MAILPIT_LOOKUP_ATTEMPTS: usize = 30;
const MAILPIT_LOOKUP_INTERVAL: Duration = Duration::from_millis(100);

pub const LISTED_APP_ORIGIN: &str = "http://localhost:3000";
pub const PREVIEW_APP_ORIGIN: &str = "https://dai-bx-web-abc123-shans-projects-a71057cb.vercel.app";
pub const TEST_TERMS_VERSION: &str = "test-terms-1";
pub const STRONG_PASSWORD: &str = "violet lantern drifts past seven quiet docks";
const TEST_TOTP_KEYS: &str = "v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

pub fn test_app_config() -> AppConfig {
    AppConfig {
        database_url: String::from("postgres://unused-in-tests"),
        allowed_app_origins: vec![
            AllowedOrigin::parse(LISTED_APP_ORIGIN).expect("listed origin should parse"),
            AllowedOrigin::parse("https://dai-bx-web-*-shans-projects-a71057cb.vercel.app")
                .expect("preview origin pattern should parse"),
        ],
        totp_keyring: TotpKeyring::parse(TEST_TOTP_KEYS, "v1")
            .expect("test TOTP keyring should parse"),
        hibp_api_base_url: String::from(UNREACHABLE_HIBP_BASE_URL),
        app_origin: String::from(LISTED_APP_ORIGIN),
        smtp_settings: SmtpSettings {
            host: String::from("127.0.0.1"),
            port: 1025,
            security: SmtpSecurity::Plain,
            credentials: None,
            mail_from: "DaiBX <no-reply@daibx.test>"
                .parse()
                .expect("test sender should parse"),
        },
        open_signups: false,
        terms_version: String::from(TEST_TERMS_VERSION),
    }
}

pub fn build_test_app(db_pool: PgPool) -> Router {
    build_test_app_with(db_pool, test_app_config())
}

pub fn build_test_app_with(db_pool: PgPool, app_config: AppConfig) -> Router {
    let mail_transport = build_mail_transport(&app_config.smtp_settings);
    app(AppState {
        db_pool,
        app_config: Arc::new(app_config),
        http_client: build_http_client(),
        mail_transport,
    })
}

pub async fn send(test_app: Router, request: Request<Body>) -> Response<Body> {
    test_app
        .oneshot(request)
        .await
        .expect("router should always answer")
}

pub async fn get(test_app: Router, request_path: &str) -> Response<Body> {
    let request = Request::get(request_path)
        .body(Body::empty())
        .expect("test request should build");
    send(test_app, request).await
}

pub async fn get_with_cookies(
    test_app: Router,
    request_path: &str,
    cookie_header: &str,
) -> Response<Body> {
    let request = Request::get(request_path)
        .header(header::COOKIE, cookie_header)
        .body(Body::empty())
        .expect("test request should build");
    send(test_app, request).await
}

pub async fn post_json(
    test_app: Router,
    request_path: &str,
    request_body: serde_json::Value,
) -> Response<Body> {
    post_json_with_cookies(test_app, request_path, request_body, "").await
}

pub async fn post_json_with_cookies(
    test_app: Router,
    request_path: &str,
    request_body: serde_json::Value,
    cookie_header: &str,
) -> Response<Body> {
    send_json_with_cookies(
        test_app,
        Method::POST,
        request_path,
        request_body,
        cookie_header,
    )
    .await
}

pub async fn patch_json_with_cookies(
    test_app: Router,
    request_path: &str,
    request_body: serde_json::Value,
    cookie_header: &str,
) -> Response<Body> {
    send_json_with_cookies(
        test_app,
        Method::PATCH,
        request_path,
        request_body,
        cookie_header,
    )
    .await
}

async fn send_json_with_cookies(
    test_app: Router,
    request_method: Method,
    request_path: &str,
    request_body: serde_json::Value,
    cookie_header: &str,
) -> Response<Body> {
    let mut request_builder = Request::builder()
        .method(request_method)
        .uri(request_path)
        .header("sec-fetch-site", "same-origin")
        .header("content-type", "application/json")
        .header("user-agent", "DaiBX feature test")
        .header("x-vercel-forwarded-for", "180.191.77.129");
    if !cookie_header.is_empty() {
        request_builder = request_builder.header(header::COOKIE, cookie_header);
    }
    let request = request_builder
        .body(Body::from(request_body.to_string()))
        .expect("test request should build");
    send(test_app, request).await
}

pub fn set_cookie_headers(response: &Response<Body>) -> Vec<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|header_value| header_value.to_str().ok())
        .map(str::to_owned)
        .collect()
}

pub fn find_set_cookie(set_cookies: &[String], cookie_name: &str) -> String {
    set_cookies
        .iter()
        .find(|set_cookie| set_cookie.starts_with(&format!("{cookie_name}=")))
        .unwrap_or_else(|| panic!("{cookie_name} should be set, got {set_cookies:?}"))
        .clone()
}

pub fn cookie_pair(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("a Set-Cookie value starts with its name and value")
        .to_owned()
}

pub fn session_cookie_header(set_cookies: &[String]) -> String {
    format!(
        "{}; {}",
        cookie_pair(&find_set_cookie(set_cookies, "__Host-daibx_access")),
        cookie_pair(&find_set_cookie(set_cookies, "__Host-daibx_refresh")),
    )
}

pub fn cookie_value<'a>(cookie_header: &'a str, cookie_name: &str) -> &'a str {
    cookie_header
        .split("; ")
        .find_map(|cookie_pair| cookie_pair.strip_prefix(&format!("{cookie_name}=")))
        .unwrap_or_else(|| panic!("{cookie_name} should be in {cookie_header}"))
}

pub struct AuthenticatorSetupStart {
    pub email: String,
    pub challenge_cookie: String,
}

pub struct SignedInPerson {
    pub email: String,
    pub session_cookies: String,
    pub totp_secret: Vec<u8>,
    pub recovery_codes: Vec<String>,
}

pub async fn start_authenticator_setup(db_pool: &PgPool) -> AuthenticatorSetupStart {
    let email = unique_email();
    let sign_up_response = post_json(
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
    assert_eq!(sign_up_response.status(), StatusCode::ACCEPTED);
    let link_token = latest_link_token(&email, 1).await;
    let verify_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/verify-email",
        json!({ "token": link_token }),
    )
    .await;
    assert_eq!(verify_response.status(), StatusCode::OK);
    let challenge_cookie = cookie_pair(&find_set_cookie(
        &set_cookie_headers(&verify_response),
        "__Host-daibx_challenge",
    ));
    AuthenticatorSetupStart {
        email,
        challenge_cookie,
    }
}

pub async fn read_pending_totp_secret(db_pool: &PgPool, challenge_cookie: &str) -> Vec<u8> {
    let setup_response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/setup",
        challenge_cookie,
    )
    .await;
    assert_eq!(setup_response.status(), StatusCode::OK);
    let manual_entry_key = response_json(setup_response).await["manualEntryKey"]
        .as_str()
        .expect("the setup should include a manual entry key")
        .replace(' ', "");
    BASE32_NOPAD
        .decode(manual_entry_key.as_bytes())
        .expect("the manual entry key should be base32")
}

fn current_totp_step() -> u64 {
    let unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock should be after 1970")
        .as_secs();
    totp_step_at(unix_seconds)
}

pub fn current_totp_code(totp_secret: &[u8]) -> String {
    totp_code_at_step(totp_secret, current_totp_step())
}

pub fn next_totp_code(totp_secret: &[u8]) -> String {
    totp_code_at_step(totp_secret, current_totp_step() + 1)
}

pub async fn sign_up_with_authenticator(db_pool: &PgPool) -> SignedInPerson {
    let setup_start = start_authenticator_setup(db_pool).await;
    let totp_secret = read_pending_totp_secret(db_pool, &setup_start.challenge_cookie).await;
    let confirm_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/setup/confirm",
        json!({ "code": current_totp_code(&totp_secret) }),
        &setup_start.challenge_cookie,
    )
    .await;
    assert_eq!(confirm_response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&confirm_response);
    let recovery_codes = response_json(confirm_response).await["recoveryCodes"]
        .as_array()
        .expect("recovery codes should be a list")
        .iter()
        .map(|recovery_code| recovery_code.as_str().expect("a code").to_owned())
        .collect();
    SignedInPerson {
        email: setup_start.email,
        session_cookies: session_cookie_header(&set_cookies),
        totp_secret,
        recovery_codes,
    }
}

pub async fn sign_in(
    db_pool: &PgPool,
    email: &str,
    password: &str,
    cookie_header: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/sign-in",
        json!({ "email": email, "password": password }),
        cookie_header,
    )
    .await
}

pub async fn sign_in_to_code_step(db_pool: &PgPool, email: &str) -> String {
    let sign_in_response = sign_in(db_pool, email, STRONG_PASSWORD, "").await;
    assert_eq!(sign_in_response.status(), StatusCode::OK);
    cookie_pair(&find_set_cookie(
        &set_cookie_headers(&sign_in_response),
        "__Host-daibx_challenge",
    ))
}

pub async fn verify_code(
    db_pool: &PgPool,
    challenge_cookie: &str,
    entered_code: &str,
    should_remember_device: bool,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/mfa/verify",
        json!({ "code": entered_code, "rememberDevice": should_remember_device }),
        challenge_cookie,
    )
    .await
}

pub async fn response_json(response: Response<Body>) -> serde_json::Value {
    let response_bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&response_bytes).expect("response body should be JSON")
}

pub fn unique_email() -> String {
    format!("{}@daibx.test", uuid::Uuid::new_v4())
}

pub async fn create_invite_code(db_pool: &PgPool) -> String {
    CreateInvite::new(db_pool)
        .execute()
        .await
        .expect("creating an invite should succeed")
        .invite_code
        .to_string()
}

#[derive(Deserialize)]
struct MailpitSearchResult {
    messages: Vec<MailpitMessageSummary>,
}

#[derive(Deserialize)]
struct MailpitMessageSummary {
    #[serde(rename = "ID")]
    id: String,
}

#[derive(Deserialize)]
struct MailpitMessage {
    #[serde(rename = "Text")]
    text_body: String,
}

async fn search_mailpit(recipient: &str) -> Vec<String> {
    let search_result: MailpitSearchResult = build_http_client()
        .get(format!("{MAILPIT_API_URL}/api/v1/search"))
        .query(&[("query", format!("to:\"{recipient}\""))])
        .send()
        .await
        .expect("Mailpit should be running for feature tests")
        .json()
        .await
        .expect("Mailpit search should return JSON");
    search_result
        .messages
        .into_iter()
        .map(|message_summary| message_summary.id)
        .collect()
}

pub async fn wait_for_email_count(recipient: &str, expected_count: usize) -> Vec<String> {
    for _ in 0..MAILPIT_LOOKUP_ATTEMPTS {
        let message_ids = search_mailpit(recipient).await;
        if message_ids.len() >= expected_count {
            return message_ids;
        }
        tokio::time::sleep(MAILPIT_LOOKUP_INTERVAL).await;
    }
    panic!("expected {expected_count} emails for {recipient} in Mailpit");
}

pub async fn count_emails_after_a_pause(recipient: &str) -> usize {
    tokio::time::sleep(Duration::from_millis(400)).await;
    search_mailpit(recipient).await.len()
}

pub async fn latest_link_token(recipient: &str, expected_email_count: usize) -> String {
    let message_ids = wait_for_email_count(recipient, expected_email_count).await;
    let latest_message: MailpitMessage = build_http_client()
        .get(format!(
            "{MAILPIT_API_URL}/api/v1/message/{}",
            message_ids[0]
        ))
        .send()
        .await
        .expect("Mailpit message should load")
        .json()
        .await
        .expect("Mailpit message should be JSON");
    latest_message
        .text_body
        .split_whitespace()
        .find_map(|text_word| {
            text_word
                .split_once("?token=")
                .map(|(_, link_token)| link_token)
        })
        .expect("the email should contain a link with a token")
        .to_owned()
}

pub async fn recorded_event_types(db_pool: &PgPool, email: &str) -> Vec<SecurityEventType> {
    let recorded_user = FindUserByEmail::new(db_pool)
        .execute(email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    ListSecurityEventsForSubject::new()
        .execute(db_pool, recorded_user.security_ref)
        .await
        .expect("listing should succeed")
        .into_iter()
        .map(|security_event| security_event.event_type)
        .collect()
}
