use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;

#[sqlx::test]
async fn deletes_a_group(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let remaining_groups = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/task-groups",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    assert_eq!(remaining_groups, json!([]));
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_deleted_group(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group = create_group(&db_pool, &owner, json!({ "name": "Mine" })).await;
    let owner_group_path = format!(
        "/task-groups/{}",
        owner_group["id"].as_str().expect("an id")
    );

    let foreign_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_group_path,
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_group_path,
        &owner.session_cookies,
    )
    .await;
    let repeated_delete = delete_with_cookies(
        build_test_app(db_pool),
        &owner_group_path,
        &owner.session_cookies,
    )
    .await;

    assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_group_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_group = create_group(&db_pool, &signed_in_person, json!({ "name": "Today" })).await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/task-groups/{}",
            today_group["id"].as_str().expect("an id")
        ),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
