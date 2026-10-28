use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::categories::update::find_category_id;
use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;

#[sqlx::test]
async fn renames_a_group_and_sets_its_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        json!({ "name": "Chinese", "categoryId": studies_category_id }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_group = response_json(response).await;
    assert_eq!(updated_group["name"], "Chinese");
    assert_eq!(updated_group["category"]["id"], studies_category_id);
    assert_eq!(updated_group["openTaskCount"], 0);
}

#[sqlx::test]
async fn clears_the_category_with_null_and_keeps_it_when_absent(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;
    let shift_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Shift", "categoryId": work_category_id }),
    )
    .await;
    let shift_group_path = format!(
        "/task-groups/{}",
        shift_group["id"].as_str().expect("an id")
    );

    let renamed_group = response_json(
        patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &shift_group_path,
            json!({ "name": "Night shift" }),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    let cleared_group = response_json(
        patch_json_with_cookies(
            build_test_app(db_pool),
            &shift_group_path,
            json!({ "categoryId": null }),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(renamed_group["category"]["id"], work_category_id);
    assert_eq!(cleared_group["name"], "Night shift");
    assert_eq!(cleared_group["category"], serde_json::Value::Null);
}

#[sqlx::test]
async fn refuses_a_bad_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        json!({ "name": "" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_group_or_category(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group = create_group(&db_pool, &owner, json!({ "name": "Mine" })).await;
    let other_person_group =
        create_group(&db_pool, &other_person, json!({ "name": "Theirs" })).await;
    let owner_category_id = find_category_id(&db_pool, &owner, "Goals").await;

    let foreign_group_response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "/task-groups/{}",
            owner_group["id"].as_str().expect("an id")
        ),
        json!({ "name": "Taken over" }),
        &other_person.session_cookies,
    )
    .await;
    let foreign_category_response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "/task-groups/{}",
            other_person_group["id"].as_str().expect("an id")
        ),
        json!({ "categoryId": owner_category_id }),
        &other_person.session_cookies,
    )
    .await;
    let malformed_id_response = patch_json_with_cookies(
        build_test_app(db_pool),
        "/task-groups/not-a-uuid",
        json!({ "name": "Nope" }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(foreign_group_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_category_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(malformed_id_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn updating_a_group_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        json!({ "name": "Tomorrow" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
