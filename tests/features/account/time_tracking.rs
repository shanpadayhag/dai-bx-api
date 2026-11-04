use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get, get_with_cookies,
    patch_json_with_cookies, response_json, sign_up_with_authenticator,
};

pub async fn patch_time_tracking(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/time-tracking",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn read_time_tracking(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/account/time-tracking",
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn a_new_account_has_the_default_settings(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    assert_eq!(
        read_time_tracking(&db_pool, &signed_in_person).await,
        json!({
            "checkInsEnabled": true,
            "checkInIntervalMinutes": 30,
            "firstCheckInMinutes": 30,
            "workingHoursStart": "16:00",
            "workingHoursEnd": "02:00",
            "trackedCategoryId": null,
        })
    );
}

#[sqlx::test]
async fn each_field_updates_on_its_own(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;

    for (field_name, field_value) in [
        ("checkInsEnabled", json!(false)),
        ("checkInIntervalMinutes", json!(5)),
        ("firstCheckInMinutes", json!(240)),
        ("workingHoursStart", json!("08:30")),
        ("workingHoursEnd", json!("23:59")),
        ("trackedCategoryId", json!(studies_category_id)),
    ] {
        let response = patch_time_tracking(
            &db_pool,
            &signed_in_person,
            json!({ field_name: field_value }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{field_name}");
        assert_eq!(response_json(response).await[field_name], field_value);
    }

    assert_eq!(
        read_time_tracking(&db_pool, &signed_in_person).await,
        json!({
            "checkInsEnabled": false,
            "checkInIntervalMinutes": 5,
            "firstCheckInMinutes": 240,
            "workingHoursStart": "08:30",
            "workingHoursEnd": "23:59",
            "trackedCategoryId": studies_category_id,
        })
    );

    let cleared_response = patch_time_tracking(
        &db_pool,
        &signed_in_person,
        json!({ "trackedCategoryId": null }),
    )
    .await;
    assert_eq!(cleared_response.status(), StatusCode::OK);
    assert_eq!(
        response_json(cleared_response).await["trackedCategoryId"],
        serde_json::Value::Null
    );
}

#[sqlx::test]
async fn refuses_an_interval_outside_5_to_240_or_a_bad_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for refused_change in [
        json!({ "checkInIntervalMinutes": 4 }),
        json!({ "checkInIntervalMinutes": 241 }),
        json!({ "firstCheckInMinutes": 0 }),
        json!({ "firstCheckInMinutes": 1000 }),
        json!({ "workingHoursStart": "24:00" }),
        json!({ "workingHoursStart": "9:30" }),
        json!({ "workingHoursEnd": "02:60" }),
        json!({ "workingHoursEnd": "02:00:00" }),
    ] {
        let response =
            patch_time_tracking(&db_pool, &signed_in_person, refused_change.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_change}"
        );
    }

    assert_eq!(
        read_time_tracking(&db_pool, &signed_in_person).await["checkInIntervalMinutes"],
        30
    );
}

#[sqlx::test]
async fn answers_not_found_for_another_person_s_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let other_category_id = find_category_id(&db_pool, &other_person, "Studies").await;

    for refused_category_id in [other_category_id.as_str(), "not-a-uuid"] {
        let response = patch_time_tracking(
            &db_pool,
            &signed_in_person,
            json!({ "trackedCategoryId": refused_category_id }),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{refused_category_id}"
        );
    }

    assert_eq!(
        read_time_tracking(&db_pool, &signed_in_person).await["trackedCategoryId"],
        serde_json::Value::Null
    );
}

#[sqlx::test]
async fn deleting_the_category_clears_the_setting(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let response = patch_time_tracking(
        &db_pool,
        &signed_in_person,
        json!({ "trackedCategoryId": studies_category_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/categories/{studies_category_id}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::OK);

    assert_eq!(
        read_time_tracking(&db_pool, &signed_in_person).await["trackedCategoryId"],
        serde_json::Value::Null
    );
}

#[sqlx::test]
async fn refuses_the_settings_without_a_session(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/account/time-tracking").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
