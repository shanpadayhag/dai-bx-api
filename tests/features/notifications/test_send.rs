use axum::http::StatusCode;
use sqlx::PgPool;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::notifications::devices::{listed_device_ids, register_device_id};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn push_service_answering(response_status: u16) -> MockServer {
    let push_service = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(response_status))
        .mount(&push_service)
        .await;
    push_service
}

async fn point_device_at_fake_push_service(
    db_pool: &PgPool,
    device_id: &str,
    push_service: &MockServer,
) {
    sqlx::query("UPDATE push_devices SET endpoint = $2 WHERE id = $1::uuid")
        .bind(device_id)
        .bind(format!("{}/push/device", push_service.uri()))
        .execute(db_pool)
        .await
        .expect("pointing the device at the fake push service should succeed");
}

async fn send_test(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    device_id: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/push-devices/{device_id}/test"),
        serde_json::json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn a_test_send_returns_200_and_records_the_delivery(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;
    let push_service = push_service_answering(201).await;
    point_device_at_fake_push_service(&db_pool, &device_id, &push_service).await;

    let response = send_test(&db_pool, &signed_in_person, &device_id).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "sent");
    let account_response = crate::support::get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/notifications",
        &signed_in_person.session_cookies,
    )
    .await;
    assert!(response_json(account_response).await["devices"][0]["lastDeliveredAt"].is_string());
}

#[sqlx::test]
async fn a_device_the_push_service_no_longer_knows_returns_410_and_is_removed(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;
    let push_service = push_service_answering(410).await;
    point_device_at_fake_push_service(&db_pool, &device_id, &push_service).await;

    let response = send_test(&db_pool, &signed_in_person, &device_id).await;

    assert_eq!(response.status(), StatusCode::GONE);
    assert!(
        listed_device_ids(&db_pool, &signed_in_person)
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn a_refused_send_returns_502_and_keeps_the_device(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;
    let push_service = push_service_answering(500).await;
    point_device_at_fake_push_service(&db_pool, &device_id, &push_service).await;

    let response = send_test(&db_pool, &signed_in_person, &device_id).await;

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        listed_device_ids(&db_pool, &signed_in_person).await,
        vec![device_id]
    );
}

#[sqlx::test]
async fn a_sixth_test_send_in_an_hour_returns_429(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;
    let push_service = push_service_answering(201).await;
    point_device_at_fake_push_service(&db_pool, &device_id, &push_service).await;
    for _ in 0..5 {
        let response = send_test(&db_pool, &signed_in_person, &device_id).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = send_test(&db_pool, &signed_in_person, &device_id).await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn a_test_send_to_another_persons_device_returns_404(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &owner, "phone").await;

    let response = send_test(&db_pool, &other_person, &device_id).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
