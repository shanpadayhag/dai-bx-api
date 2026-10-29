use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, post_json, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn create_group(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/task-groups",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

#[sqlx::test]
async fn creates_a_group_with_just_a_trimmed_name_and_no_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let created_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "  DaiBX  ", "categoryId": "ignored-by-the-api" }),
    )
    .await;

    let mut response_keys: Vec<&String> = created_group
        .as_object()
        .expect("a group object")
        .keys()
        .collect();
    response_keys.sort();
    assert_eq!(
        response_keys,
        vec!["id", "name", "openTaskCount", "taskCount"]
    );
    assert_eq!(created_group["name"], "DaiBX");
    assert_eq!(created_group["openTaskCount"], 0);
}

#[sqlx::test]
async fn allows_two_groups_with_the_same_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let first_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;
    let second_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    assert_ne!(first_group["id"], second_group["id"]);
}

#[sqlx::test]
async fn refuses_a_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for rejected_name in ["  ", &"a".repeat(201)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/task-groups",
            json!({ "name": rejected_name }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a group name is 1 to 200 characters" })
        );
    }
}

#[sqlx::test]
async fn creating_a_group_needs_a_session(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool),
        "/task-groups",
        json!({ "name": "Today" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
