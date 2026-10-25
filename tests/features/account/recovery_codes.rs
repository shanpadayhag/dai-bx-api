use axum::body::Body;
use axum::http::{Response, StatusCode};
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    STRONG_PASSWORD, SignedInPerson, build_test_app, next_totp_code, post_json_with_cookies,
    recorded_event_types, response_json, sign_in_to_code_step, sign_up_with_authenticator,
    verify_code,
};

async fn regenerate_codes(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    password: &str,
    entered_code: &str,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/recovery-codes",
        json!({ "password": password, "code": entered_code }),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn code_step_status(db_pool: &PgPool, email: &str, entered_code: &str) -> StatusCode {
    let challenge_cookie = sign_in_to_code_step(db_pool, email).await;
    verify_code(db_pool, &challenge_cookie, entered_code, false)
        .await
        .status()
}

#[sqlx::test]
async fn regenerating_returns_ten_new_codes_and_kills_every_old_one(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = regenerate_codes(
        &db_pool,
        &signed_in_person,
        STRONG_PASSWORD,
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let new_codes: Vec<String> = response_json(response).await["recoveryCodes"]
        .as_array()
        .expect("recovery codes should be a list")
        .iter()
        .map(|recovery_code| recovery_code.as_str().expect("a code").to_owned())
        .collect();
    assert_eq!(new_codes.len(), 10);
    assert!(
        new_codes
            .iter()
            .all(|new_code| !signed_in_person.recovery_codes.contains(new_code))
    );
    assert_eq!(
        code_step_status(
            &db_pool,
            &signed_in_person.email,
            &signed_in_person.recovery_codes[0]
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        code_step_status(&db_pool, &signed_in_person.email, &new_codes[0]).await,
        StatusCode::OK
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::RecoveryCodesRegenerated)
    );
}

#[sqlx::test]
async fn regenerating_needs_the_password_and_a_code(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let wrong_password_response = regenerate_codes(
        &db_pool,
        &signed_in_person,
        "seven quiet docks drift past a violet lantern",
        &next_totp_code(&signed_in_person.totp_secret),
    )
    .await;
    let wrong_code_response =
        regenerate_codes(&db_pool, &signed_in_person, STRONG_PASSWORD, "000000").await;

    assert_eq!(
        wrong_password_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        wrong_code_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        code_step_status(
            &db_pool,
            &signed_in_person.email,
            &signed_in_person.recovery_codes[0]
        )
        .await,
        StatusCode::OK
    );
}
