use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, get, get_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator, test_app_config,
};

#[sqlx::test]
async fn a_new_account_has_every_switch_on_and_the_public_key(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/notifications",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "applicationServerKey": test_app_config().vapid_settings.application_server_key,
            "checkIns": true,
            "scheduleRuns": true,
            "blocksStarting": true,
            "quietOutsideWorkingHours": true,
            "devices": [],
        })
    );
}

#[sqlx::test]
async fn each_switch_updates_on_its_own(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for field_name in [
        "checkIns",
        "scheduleRuns",
        "blocksStarting",
        "quietOutsideWorkingHours",
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/account/notifications",
            json!({ field_name: false }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK, "{field_name}");
        assert_eq!(response_json(response).await[field_name], false);
    }
}

#[sqlx::test]
async fn an_unknown_or_wrongly_typed_field_returns_422(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for request_body in [json!({ "dueDates": true }), json!({ "checkIns": "no" })] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/account/notifications",
            request_body.clone(),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{request_body}"
        );
    }
}

#[sqlx::test]
async fn reading_without_a_session_returns_401(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/account/notifications").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
