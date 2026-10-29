use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;

#[sqlx::test]
async fn renames_a_group(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        json!({ "name": "  Chinese  " }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_group = response_json(response).await;
    assert_eq!(updated_group["name"], "Chinese");
    assert_eq!(updated_group["openTaskCount"], 0);
    assert!(updated_group.get("category").is_none());
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
async fn answers_not_found_for_someone_else_s_or_a_malformed_group(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group = create_group(&db_pool, &owner, json!({ "name": "Mine" })).await;

    for group_path in [
        format!(
            "/task-groups/{}",
            owner_group["id"].as_str().expect("an id")
        ),
        String::from("/task-groups/not-a-uuid"),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &group_path,
            json!({ "name": "Taken over" }),
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{group_path}");
    }
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
