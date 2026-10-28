use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, get, get_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, task_id_named};

#[sqlx::test]
async fn returns_the_group_and_its_tasks_as_a_flat_list(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let parent_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Parent", None).await;
    let parent_task_id = task_id_named(&parent_tasks, "Parent");
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Child",
        Some(&parent_task_id),
    )
    .await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        &format!("/task-groups/{task_group_id}/tasks"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let group_tasks = response_json(response).await;
    assert_eq!(
        group_tasks["group"],
        json!({ "id": task_group_id, "name": "Today", "category": null })
    );
    assert_eq!(group_tasks["tasks"].as_array().expect("a list").len(), 2);
    assert_eq!(group_tasks["openCount"], 2);
    assert_eq!(group_tasks["doneCount"], 0);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_malformed_group(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group_id = create_group_id(&db_pool, &owner).await;

    for group_path in [
        format!("/task-groups/{owner_group_id}/tasks"),
        String::from("/task-groups/not-a-uuid/tasks"),
    ] {
        let response = get_with_cookies(
            build_test_app(db_pool.clone()),
            &group_path,
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn reading_tasks_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;

    let response = get(
        build_test_app(db_pool),
        &format!("/task-groups/{task_group_id}/tasks"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
