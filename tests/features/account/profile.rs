use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, get, get_with_cookies, patch_json_with_cookies, post_json_with_cookies,
    response_json, session_cookie_header, set_cookie_headers, sign_in_to_code_step,
    sign_up_with_authenticator, verify_code,
};

#[sqlx::test]
async fn get_account_returns_the_profile_and_settings(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "name": "Shan Padayhag",
            "email": signed_in_person.email,
            "accentColor": "blue",
            "theme": "system",
            "security": { "recoveryCodesRemaining": 10, "activeSessionCount": 1 },
        })
    );
}

#[sqlx::test]
async fn get_account_counts_unused_recovery_codes_and_live_sessions(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let challenge_cookie = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    verify_code(
        &db_pool,
        &challenge_cookie,
        &signed_in_person.recovery_codes[0],
        false,
    )
    .await;
    let ended_challenge = sign_in_to_code_step(&db_pool, &signed_in_person.email).await;
    let ended_sign_in = verify_code(
        &db_pool,
        &ended_challenge,
        &signed_in_person.recovery_codes[1],
        false,
    )
    .await;
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/auth/sign-out",
        json!({}),
        &session_cookie_header(&set_cookie_headers(&ended_sign_in)),
    )
    .await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(
        response_json(response).await["security"],
        json!({ "recoveryCodesRemaining": 8, "activeSessionCount": 2 })
    );
}

#[sqlx::test]
async fn get_account_returns_401_without_a_session(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/account").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn patch_account_changes_the_name_accent_color_and_theme(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account",
        json!({ "name": "  Shan P.  ", "accentColor": "teal", "theme": "dark" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let expected_profile = json!({
        "name": "Shan P.",
        "email": signed_in_person.email,
        "accentColor": "teal",
        "theme": "dark",
    });
    assert_eq!(response_json(response).await, expected_profile);
    let reread_response = get_with_cookies(
        build_test_app(db_pool),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;
    let mut reread_profile = response_json(reread_response).await;
    reread_profile
        .as_object_mut()
        .expect("the account should be an object")
        .remove("security");
    assert_eq!(reread_profile, expected_profile);
}

#[sqlx::test]
async fn patch_account_changes_only_the_fields_sent(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        "/account",
        json!({ "theme": "light" }),
        &signed_in_person.session_cookies,
    )
    .await;

    let changed_profile = response_json(response).await;
    assert_eq!(changed_profile["theme"], "light");
    assert_eq!(changed_profile["accentColor"], "blue");
    assert_eq!(changed_profile["name"], "Shan Padayhag");
}

#[sqlx::test]
async fn patch_account_returns_422_for_an_unknown_theme_accent_color_or_empty_name(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for invalid_body in [
        json!({ "theme": "neon" }),
        json!({ "accentColor": "pink" }),
        json!({ "name": "   " }),
        json!({ "name": "x".repeat(101) }),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/account",
            invalid_body.clone(),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid_body}"
        );
    }
    let unchanged_response = get_with_cookies(
        build_test_app(db_pool),
        "/account",
        &signed_in_person.session_cookies,
    )
    .await;
    let unchanged_profile = response_json(unchanged_response).await;
    assert_eq!(unchanged_profile["theme"], "system");
    assert_eq!(unchanged_profile["accentColor"], "blue");
}
