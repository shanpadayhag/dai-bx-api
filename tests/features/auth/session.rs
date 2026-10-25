use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, get, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn session_status(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> StatusCode {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/session",
        &signed_in_person.session_cookies,
    )
    .await
    .status()
}

async fn age_sessions(db_pool: &PgPool, update_statement: &'static str) {
    sqlx::query(update_statement)
        .execute(db_pool)
        .await
        .expect("ageing the session should succeed");
}

#[sqlx::test]
async fn get_session_returns_200_with_the_signed_in_person(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/auth/session",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "user": {
                "name": "Shan Padayhag",
                "email": signed_in_person.email,
                "accentColor": "blue",
                "theme": "system",
            }
        })
    );
}

#[sqlx::test]
async fn get_session_returns_401_without_a_session(db_pool: PgPool) {
    let response_without_cookie = get(build_test_app(db_pool.clone()), "/auth/session").await;
    let response_with_unknown_token = get_with_cookies(
        build_test_app(db_pool),
        "/auth/session",
        "__Host-daibx_access=qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw",
    )
    .await;

    assert_eq!(response_without_cookie.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_with_unknown_token.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn get_session_returns_401_for_a_session_idle_over_7_days(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    age_sessions(
        &db_pool,
        "UPDATE sessions SET last_seen_at = now() - interval '7 days 1 minute'",
    )
    .await;

    assert_eq!(
        session_status(&db_pool, &signed_in_person).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn get_session_returns_401_for_a_session_older_than_30_days(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    age_sessions(
        &db_pool,
        "UPDATE sessions SET created_at = now() - interval '30 days 1 minute'",
    )
    .await;

    assert_eq!(
        session_status(&db_pool, &signed_in_person).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn get_session_returns_401_for_a_revoked_session_or_an_expired_access_token(db_pool: PgPool) {
    let revoked_person = sign_up_with_authenticator(&db_pool).await;
    age_sessions(&db_pool, "UPDATE sessions SET revoked_at = now()").await;
    let expired_person = sign_up_with_authenticator(&db_pool).await;
    age_sessions(
        &db_pool,
        "UPDATE session_tokens SET expires_at = now() - interval '1 second' WHERE kind = 'access'",
    )
    .await;

    assert_eq!(
        session_status(&db_pool, &revoked_person).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        session_status(&db_pool, &expired_person).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn get_session_does_not_accept_the_refresh_token_as_an_access_token(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let refresh_value = signed_in_person
        .session_cookies
        .split("; ")
        .find_map(|cookie_pair| cookie_pair.strip_prefix("__Host-daibx_refresh="))
        .expect("the refresh cookie should be present");

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/auth/session",
        &format!("__Host-daibx_access={refresh_value}"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
