use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, cookie_value, find_set_cookie, get_with_cookies, post_json_with_cookies,
    recorded_event_types, response_json, session_cookie_header, set_cookie_headers,
    sign_up_with_authenticator,
};

async fn refresh(db_pool: &PgPool, cookie_header: &str) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/refresh",
        json!({}),
        cookie_header,
    )
    .await
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

async fn run_statement(db_pool: &PgPool, statement: &'static str) {
    sqlx::query(statement)
        .execute(db_pool)
        .await
        .expect("the test statement should succeed");
}

fn refresh_max_age_seconds(set_cookies: &[String]) -> i64 {
    find_set_cookie(set_cookies, "__Host-daibx_refresh")
        .split("; ")
        .find_map(|cookie_attribute| cookie_attribute.strip_prefix("Max-Age="))
        .and_then(|max_age_text| max_age_text.parse().ok())
        .expect("the refresh cookie should have a Max-Age")
}

#[sqlx::test]
async fn refresh_rotates_both_tokens_and_moves_last_seen(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    run_statement(
        &db_pool,
        "UPDATE sessions SET last_seen_at = now() - interval '1 day'",
    )
    .await;

    let response = refresh(&db_pool, &signed_in_person.session_cookies).await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    let renewed_cookies = session_cookie_header(&set_cookies);
    for cookie_name in ["__Host-daibx_access", "__Host-daibx_refresh"] {
        assert_ne!(
            cookie_value(&renewed_cookies, cookie_name),
            cookie_value(&signed_in_person.session_cookies, cookie_name)
        );
    }
    assert_eq!(refresh_max_age_seconds(&set_cookies), 604_800);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "refreshed" })
    );
    assert_eq!(
        session_status(&db_pool, &renewed_cookies).await,
        StatusCode::OK
    );
    let is_recently_seen: bool = sqlx::query_scalar(
        "SELECT bool_and(last_seen_at > now() - interval '1 minute') FROM sessions",
    )
    .fetch_one(&db_pool)
    .await
    .expect("reading the session should succeed");
    assert!(is_recently_seen);
}

#[sqlx::test]
async fn refresh_accepts_the_previous_token_once_more_within_30_seconds(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let first_response = refresh(&db_pool, &signed_in_person.session_cookies).await;
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = refresh(&db_pool, &signed_in_person.session_cookies).await;

    assert_eq!(second_response.status(), StatusCode::OK);
    let second_cookies = session_cookie_header(&set_cookie_headers(&second_response));
    assert_eq!(
        session_status(&db_pool, &second_cookies).await,
        StatusCode::OK
    );
}

#[sqlx::test]
async fn refresh_treats_a_third_use_of_a_replaced_token_as_reuse(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let first_response = refresh(&db_pool, &signed_in_person.session_cookies).await;
    let first_cookies = session_cookie_header(&set_cookie_headers(&first_response));
    refresh(&db_pool, &signed_in_person.session_cookies).await;

    let third_response = refresh(&db_pool, &signed_in_person.session_cookies).await;

    assert_eq!(third_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        session_status(&db_pool, &first_cookies).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn refresh_revokes_the_session_when_a_token_is_reused_after_30_seconds(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let first_response = refresh(&db_pool, &signed_in_person.session_cookies).await;
    let renewed_cookies = session_cookie_header(&set_cookie_headers(&first_response));
    run_statement(
        &db_pool,
        "UPDATE session_tokens SET replaced_at = now() - interval '31 seconds' WHERE replaced_at IS NOT NULL",
    )
    .await;

    let reused_response = refresh(&db_pool, &signed_in_person.session_cookies).await;

    assert_eq!(reused_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        session_status(&db_pool, &renewed_cookies).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        refresh(&db_pool, &renewed_cookies).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::RefreshTokenReuseDetected)
    );
}

#[sqlx::test]
async fn refresh_returns_401_without_a_known_refresh_token(db_pool: PgPool) {
    let missing_response = refresh(&db_pool, "").await;
    let unknown_response = refresh(
        &db_pool,
        "__Host-daibx_refresh=qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw",
    )
    .await;

    assert_eq!(missing_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refresh_does_not_accept_an_access_token(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let access_value = cookie_value(&signed_in_person.session_cookies, "__Host-daibx_access");

    let response = refresh(&db_pool, &format!("__Host-daibx_refresh={access_value}")).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refresh_returns_401_for_a_session_idle_over_7_days_or_older_than_30_days(db_pool: PgPool) {
    let idle_person = sign_up_with_authenticator(&db_pool).await;
    run_statement(
        &db_pool,
        "UPDATE sessions SET last_seen_at = now() - interval '7 days 1 minute'",
    )
    .await;
    let old_person = sign_up_with_authenticator(&db_pool).await;
    run_statement(
        &db_pool,
        "UPDATE sessions SET created_at = now() - interval '30 days 1 minute' WHERE last_seen_at > now() - interval '1 day'",
    )
    .await;

    assert_eq!(
        refresh(&db_pool, &idle_person.session_cookies)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        refresh(&db_pool, &old_person.session_cookies)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn refresh_caps_the_new_refresh_token_at_the_30_day_session_limit(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    run_statement(
        &db_pool,
        "UPDATE sessions SET created_at = now() - interval '29 days'",
    )
    .await;

    let response = refresh(&db_pool, &signed_in_person.session_cookies).await;

    assert_eq!(response.status(), StatusCode::OK);
    let refresh_max_age = refresh_max_age_seconds(&set_cookie_headers(&response));
    assert!(
        (86_000..=86_400).contains(&refresh_max_age),
        "{refresh_max_age}"
    );
}
