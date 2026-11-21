use axum::http::StatusCode;
use axum::response::Response;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get_with_cookies, post_json,
    post_json_with_cookies, recorded_event_types, response_json, sign_up_with_authenticator,
};

pub const SAMPLE_P256DH: &str =
    "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
pub const SAMPLE_AUTH_SECRET: &str = "tBHItJI5svbpez7KI4CCXg";

pub fn device_body(endpoint_suffix: &str) -> Value {
    json!({
        "endpoint": format!("https://fcm.googleapis.com/fcm/send/{endpoint_suffix}"),
        "keys": { "p256dh": SAMPLE_P256DH, "auth": SAMPLE_AUTH_SECRET },
        "deviceName": "iPhone · Safari",
        "timeZone": "Asia/Manila",
    })
}

pub async fn register_device(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: Value,
) -> Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/push-devices",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn register_device_id(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    endpoint_suffix: &str,
) -> String {
    let response = register_device(db_pool, signed_in_person, device_body(endpoint_suffix)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["id"]
        .as_str()
        .expect("the device should have an id")
        .to_owned()
}

pub async fn listed_device_ids(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> Vec<String> {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/notifications",
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["devices"]
        .as_array()
        .expect("devices should be a list")
        .iter()
        .map(|listed_device| listed_device["id"].as_str().expect("an id").to_owned())
        .collect()
}

#[sqlx::test]
async fn registering_a_device_returns_201_and_lists_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = register_device(&db_pool, &signed_in_person, device_body("phone")).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let registered_device = response_json(response).await;
    assert_eq!(registered_device["deviceName"], "iPhone · Safari");
    assert_eq!(registered_device["lastDeliveredAt"], Value::Null);
    assert_eq!(
        listed_device_ids(&db_pool, &signed_in_person).await,
        vec![registered_device["id"].as_str().expect("an id").to_owned()]
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PushDeviceAdded)
    );
}

#[sqlx::test]
async fn registering_the_same_endpoint_again_returns_200_and_keeps_one_device(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;
    let mut renamed_body = device_body("phone");
    renamed_body["deviceName"] = json!("iPad · Safari");

    let response = register_device(&db_pool, &signed_in_person, renamed_body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_device = response_json(response).await;
    assert_eq!(updated_device["id"], device_id);
    assert_eq!(updated_device["deviceName"], "iPad · Safari");
    assert_eq!(
        listed_device_ids(&db_pool, &signed_in_person).await.len(),
        1
    );
}

#[sqlx::test]
async fn an_endpoint_registered_by_another_account_moves_to_the_new_one(db_pool: PgPool) {
    let first_person = sign_up_with_authenticator(&db_pool).await;
    let second_person = sign_up_with_authenticator(&db_pool).await;
    register_device_id(&db_pool, &first_person, "shared-browser").await;

    let response = register_device(&db_pool, &second_person, device_body("shared-browser")).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(listed_device_ids(&db_pool, &first_person).await.is_empty());
    assert_eq!(listed_device_ids(&db_pool, &second_person).await.len(), 1);
    assert!(
        recorded_event_types(&db_pool, &first_person.email)
            .await
            .contains(&SecurityEventType::PushDeviceRemoved)
    );
}

#[sqlx::test]
async fn an_eleventh_device_returns_409(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for device_number in 0..10 {
        register_device_id(
            &db_pool,
            &signed_in_person,
            &format!("device-{device_number}"),
        )
        .await;
    }

    let response = register_device(&db_pool, &signed_in_person, device_body("device-10")).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        listed_device_ids(&db_pool, &signed_in_person).await.len(),
        10
    );
}

#[sqlx::test]
async fn each_invalid_field_returns_422_at_its_path(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for (field_path, field_value, error_path) in [
        ("endpoint", json!("https://evil.example/push"), "endpoint"),
        (
            "endpoint",
            json!("http://fcm.googleapis.com/fcm/send/x"),
            "endpoint",
        ),
        ("p256dh", json!("bm90IGEga2V5"), "keys.p256dh"),
        ("auth", json!("c2hvcnQ"), "keys.auth"),
        ("deviceName", json!("   "), "deviceName"),
        ("deviceName", json!("x".repeat(61)), "deviceName"),
        ("timeZone", json!("Mars/Olympus"), "timeZone"),
    ] {
        let mut invalid_body = device_body("phone");
        match field_path {
            "p256dh" | "auth" => invalid_body["keys"][field_path] = field_value,
            _ => invalid_body[field_path] = field_value,
        }

        let response = register_device(&db_pool, &signed_in_person, invalid_body).await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field_path}"
        );
        assert_eq!(response_json(response).await["path"], error_path);
    }
    assert!(
        listed_device_ids(&db_pool, &signed_in_person)
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn registering_without_a_session_returns_401(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool.clone()),
        "/push-devices",
        device_body("phone"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn removing_a_device_returns_200_and_stops_listing_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &signed_in_person, "phone").await;

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/push-devices/{device_id}"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "deleted");
    assert!(
        listed_device_ids(&db_pool, &signed_in_person)
            .await
            .is_empty()
    );
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::PushDeviceRemoved)
    );
}

#[sqlx::test]
async fn removing_another_persons_device_returns_404_and_keeps_it(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let device_id = register_device_id(&db_pool, &owner, "phone").await;

    for removed_path in [
        format!("/push-devices/{device_id}"),
        String::from("/push-devices/not-a-uuid"),
    ] {
        let response = delete_with_cookies(
            build_test_app(db_pool.clone()),
            &removed_path,
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{removed_path}");
    }
    assert_eq!(listed_device_ids(&db_pool, &owner).await, vec![device_id]);
}
