use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::actions::list_security_events_for_subject::ListSecurityEventsForSubject;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, find_set_cookie, get_with_cookies,
    latest_link_token, next_totp_code, post_json, post_json_with_cookies, response_json,
    set_cookie_headers, sign_in, sign_in_to_code_step, sign_up_with_authenticator, verify_code,
};

const USER_OWNED_TABLES: [&str; 7] = [
    "users",
    "sessions",
    "session_tokens",
    "remembered_devices",
    "recovery_codes",
    "email_tokens",
    "auth_challenges",
];

async fn delete_account(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    entered_code: &str,
    confirmation_text: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/delete",
        json!({
            "password": STRONG_PASSWORD,
            "code": entered_code,
            "confirmation": confirmation_text,
        }),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn count_rows(db_pool: &PgPool, table_name: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table_name}"
    )))
    .fetch_one(db_pool)
    .await
    .expect("counting should succeed")
}

async fn seed_everything_an_account_owns(db_pool: &PgPool, signed_in_person: &SignedInPerson) {
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    let remembered_response = verify_code(
        db_pool,
        &challenge_cookie,
        &next_totp_code(&signed_in_person.totp_secret),
        true,
    )
    .await;
    assert_eq!(remembered_response.status(), StatusCode::OK);
    sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    let reset_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/password-reset",
        json!({ "email": signed_in_person.email }),
    )
    .await;
    assert_eq!(reset_response.status(), StatusCode::ACCEPTED);
    latest_link_token(&signed_in_person.email, 2).await;
    for table_name in USER_OWNED_TABLES {
        assert!(
            count_rows(db_pool, table_name).await > 0,
            "{table_name} should be seeded"
        );
    }
}

#[sqlx::test]
async fn deleting_removes_every_owned_row_and_keeps_an_orphaned_security_log(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    seed_everything_an_account_owns(&db_pool, &signed_in_person).await;
    let security_ref = FindUserByEmail::new(&db_pool)
        .execute(&signed_in_person.email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist")
        .security_ref;

    let response = delete_account(
        &db_pool,
        &signed_in_person,
        &signed_in_person.recovery_codes[0],
        "delete my account",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookies = set_cookie_headers(&response);
    for cleared_cookie in [
        "__Host-daibx_access",
        "__Host-daibx_refresh",
        "__Host-daibx_device",
    ] {
        assert!(find_set_cookie(&set_cookies, cleared_cookie).contains("Max-Age=0"));
    }
    assert_eq!(
        response_json(response).await,
        json!({ "status": "account_deleted" })
    );
    for table_name in USER_OWNED_TABLES {
        assert_eq!(count_rows(&db_pool, table_name).await, 0, "{table_name}");
    }
    let unlinked_invite_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invites WHERE used_at IS NOT NULL AND used_by_user_id IS NULL",
    )
    .fetch_one(&db_pool)
    .await
    .expect("counting should succeed");
    assert_eq!(unlinked_invite_count, 1);
    let remaining_event_types: Vec<SecurityEventType> = ListSecurityEventsForSubject::new()
        .execute(&db_pool, security_ref)
        .await
        .expect("listing should succeed")
        .into_iter()
        .map(|security_event| security_event.event_type)
        .collect();
    assert!(remaining_event_types.contains(&SecurityEventType::SignUp));
    assert!(remaining_event_types.contains(&SecurityEventType::AccountDeleted));
    assert_eq!(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            "/auth/session",
            &signed_in_person.session_cookies
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sign_in(&db_pool, &signed_in_person.email, STRONG_PASSWORD, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn deleting_returns_422_for_the_wrong_confirmation_text(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for wrong_text in ["delete account", "Delete My Account", ""] {
        let response = delete_account(
            &db_pool,
            &signed_in_person,
            &signed_in_person.recovery_codes[0],
            wrong_text,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{wrong_text:?}"
        );
        assert_eq!(
            response_json(response).await["error"],
            "type \"delete my account\" to confirm"
        );
    }
    assert_eq!(count_rows(&db_pool, "users").await, 1);
}

#[sqlx::test]
async fn deleting_needs_a_correct_code(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = delete_account(&db_pool, &signed_in_person, "000000", "delete my account").await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(count_rows(&db_pool, "users").await, 1);
}
