use axum::http::StatusCode;
use daibx_api::features::security_log::actions::list_security_events_for_subject::ListSecurityEventsForSubject;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::users::actions::find_user_by_email::FindUserByEmail;
use serde_json::json;
use sqlx::PgPool;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    STRONG_PASSWORD, TEST_TERMS_VERSION, build_test_app, build_test_app_with,
    count_emails_after_a_pause, create_invite_code, latest_link_token, post_json, response_json,
    test_app_config, unique_email,
};

fn sign_up_body(invite_code: &str, email: &str, password: &str) -> serde_json::Value {
    json!({
        "inviteCode": invite_code,
        "name": "Shan Padayhag",
        "email": email,
        "password": password,
    })
}

#[sqlx::test]
async fn sign_up_returns_202_creates_an_unverified_account_and_emails_a_link(db_pool: PgPool) {
    let invite_code = create_invite_code(&db_pool).await;
    let email = unique_email();

    let response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        sign_up_body(&invite_code, &email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(response).await["status"],
        "verification_email_sent"
    );
    let created_user = FindUserByEmail::new(&db_pool)
        .execute(&email)
        .await
        .expect("lookup should succeed")
        .expect("the account should exist");
    assert!(!created_user.is_email_verified);
    assert_eq!(created_user.name, "Shan Padayhag");
    assert_eq!(created_user.terms_version, TEST_TERMS_VERSION);
    assert!(created_user.password_hash.starts_with("$argon2id$"));
    assert_eq!(latest_link_token(&email, 1).await.len(), 43);
    let recorded_event_types: Vec<SecurityEventType> = ListSecurityEventsForSubject::new()
        .execute(&db_pool, created_user.security_ref)
        .await
        .expect("listing should succeed")
        .into_iter()
        .map(|security_event| security_event.event_type)
        .collect();
    assert_eq!(
        recorded_event_types,
        vec![SecurityEventType::SignUp, SecurityEventType::InviteUsed]
    );
}

#[sqlx::test]
async fn sign_up_returns_422_for_each_invalid_field(db_pool: PgPool) {
    let invite_code = create_invite_code(&db_pool).await;
    let invalid_bodies = [
        json!({ "inviteCode": invite_code, "name": "  ", "email": unique_email(), "password": STRONG_PASSWORD }),
        json!({ "inviteCode": invite_code, "name": "x".repeat(101), "email": unique_email(), "password": STRONG_PASSWORD }),
        json!({ "inviteCode": invite_code, "name": "Shan", "email": "not an email", "password": STRONG_PASSWORD }),
        json!({ "inviteCode": invite_code, "name": "Shan", "email": unique_email(), "password": "too short" }),
        json!({ "inviteCode": invite_code, "name": "Shan", "email": unique_email(), "password": "passwordpassword" }),
        json!({ "inviteCode": "", "name": "Shan", "email": unique_email(), "password": STRONG_PASSWORD }),
    ];

    let mut error_messages = Vec::new();
    for invalid_body in invalid_bodies {
        let response = post_json(
            build_test_app(db_pool.clone()),
            "/auth/sign-up",
            invalid_body,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        error_messages.push(response_json(response).await["error"].to_string());
    }

    error_messages.sort();
    error_messages.dedup();
    assert!(
        error_messages.len() >= 5,
        "each field should explain itself: {error_messages:?}"
    );
}

#[sqlx::test]
async fn sign_up_returns_422_with_one_message_for_unknown_used_or_malformed_invites(
    db_pool: PgPool,
) {
    let used_invite_code = create_invite_code(&db_pool).await;
    post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        sign_up_body(&used_invite_code, &unique_email(), STRONG_PASSWORD),
    )
    .await;

    let mut error_messages = Vec::new();
    for refused_code in ["K7QM-2XRD-9FTB-0011", used_invite_code.as_str(), "nonsense"] {
        let response = post_json(
            build_test_app(db_pool.clone()),
            "/auth/sign-up",
            sign_up_body(refused_code, &unique_email(), STRONG_PASSWORD),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        error_messages.push(response_json(response).await["error"].to_string());
    }

    error_messages.dedup();
    assert_eq!(
        error_messages.len(),
        1,
        "messages differ: {error_messages:?}"
    );
}

#[sqlx::test]
async fn sign_up_answers_a_taken_email_exactly_like_a_new_one(db_pool: PgPool) {
    let email = unique_email();
    let first_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        sign_up_body(&create_invite_code(&db_pool).await, &email, STRONG_PASSWORD),
    )
    .await;
    let second_invite_code = create_invite_code(&db_pool).await;

    let repeated_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        sign_up_body(&second_invite_code, &email.to_uppercase(), STRONG_PASSWORD),
    )
    .await;

    assert_eq!(first_response.status(), StatusCode::ACCEPTED);
    assert_eq!(repeated_response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response_json(first_response).await,
        response_json(repeated_response).await
    );
    assert_eq!(count_emails_after_a_pause(&email).await, 1);
    let untouched_invite_response = post_json(
        build_test_app(db_pool.clone()),
        "/auth/sign-up",
        sign_up_body(&second_invite_code, &unique_email(), STRONG_PASSWORD),
    )
    .await;
    assert_eq!(untouched_invite_response.status(), StatusCode::ACCEPTED);
}

#[sqlx::test]
async fn sign_up_skips_the_invite_when_open_signups_is_on(db_pool: PgPool) {
    let open_app_config = daibx_api::core::config::app_config::AppConfig {
        open_signups: true,
        ..test_app_config()
    };

    let response = post_json(
        build_test_app_with(db_pool, open_app_config),
        "/auth/sign-up",
        json!({ "name": "Shan", "email": unique_email(), "password": STRONG_PASSWORD }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[sqlx::test]
async fn sign_up_returns_422_for_a_password_found_in_breaches(db_pool: PgPool) {
    let breached_password = "an otherwise fine long passphrase";
    let password_sha1_hex = {
        use sha1::{Digest, Sha1};
        data_encoding::HEXUPPER.encode(&Sha1::digest(breached_password.as_bytes()))
    };
    let hibp_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("{}:9\r\n", &password_sha1_hex[5..])),
        )
        .mount(&hibp_server)
        .await;
    let hibp_app_config = daibx_api::core::config::app_config::AppConfig {
        hibp_api_base_url: hibp_server.uri(),
        ..test_app_config()
    };

    let response = post_json(
        build_test_app_with(db_pool.clone(), hibp_app_config),
        "/auth/sign-up",
        sign_up_body(
            &create_invite_code(&db_pool).await,
            &unique_email(),
            breached_password,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        response_json(response).await["error"]
            .as_str()
            .is_some_and(|error_message| error_message.contains("breach"))
    );
}
