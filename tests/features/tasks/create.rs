use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, post_json, post_json_with_cookies,
    response_json, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;
use crate::tasks::completion::set_task_done;

pub async fn create_group_id(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    create_group(db_pool, signed_in_person, json!({ "name": "Today" })).await["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

pub async fn add_task(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_group_id: &str,
    task_name: &str,
    parent_task_id: Option<&str>,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/task-groups/{task_group_id}/tasks"),
        json!({ "name": task_name, "parentId": parent_task_id }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub fn task_id_named(group_tasks: &serde_json::Value, task_name: &str) -> String {
    find_task_named(group_tasks, task_name)["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

pub fn find_task_named<'a>(
    group_tasks: &'a serde_json::Value,
    task_name: &str,
) -> &'a serde_json::Value {
    group_tasks["tasks"]
        .as_array()
        .expect("tasks should be a list")
        .iter()
        .find(|task_json| task_json["name"] == task_name)
        .expect("the task should be listed")
}

pub async fn add_task_chain(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_group_id: &str,
    chain_length: usize,
) -> String {
    let mut parent_task_id: Option<String> = None;
    for chain_index in 0..chain_length {
        let task_name = format!("Level {}", chain_index + 1);
        let group_tasks = add_task(
            db_pool,
            signed_in_person,
            task_group_id,
            &task_name,
            parent_task_id.as_deref(),
        )
        .await;
        parent_task_id = Some(task_id_named(&group_tasks, &task_name));
    }
    parent_task_id.expect("the chain should have at least one task")
}

#[sqlx::test]
async fn adds_a_top_level_task_last_and_returns_the_group_s_tasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    add_task(&db_pool, &signed_in_person, &task_group_id, "First", None).await;

    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "  Second  ",
        None,
    )
    .await;

    assert_eq!(group_tasks["group"]["id"], task_group_id);
    assert_eq!(group_tasks["group"]["name"], "Today");
    assert_eq!(group_tasks["openCount"], 2);
    assert_eq!(group_tasks["doneCount"], 0);
    let second_task = find_task_named(&group_tasks, "Second");
    assert_eq!(second_task["parentId"], serde_json::Value::Null);
    assert_eq!(second_task["position"], 1);
    assert_eq!(second_task["isDone"], false);
    assert_eq!(second_task["estimateMinutes"], serde_json::Value::Null);
}

#[sqlx::test]
async fn adds_subtasks_last_under_their_parent(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let parent_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Parent", None).await;
    let parent_task_id = task_id_named(&parent_tasks, "Parent");
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Child A",
        Some(&parent_task_id),
    )
    .await;

    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Child B",
        Some(&parent_task_id),
    )
    .await;

    let second_child = find_task_named(&group_tasks, "Child B");
    assert_eq!(second_child["parentId"], parent_task_id);
    assert_eq!(second_child["position"], 1);
}

#[sqlx::test]
async fn allows_32_levels_and_refuses_the_33rd(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let deepest_task_id = add_task_chain(&db_pool, &signed_in_person, &task_group_id, 32).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/task-groups/{task_group_id}/tasks"),
        json!({ "name": "Level 33", "parentId": deepest_task_id }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "tasks nest at most 32 levels deep" })
    );
}

#[sqlx::test]
async fn reopens_done_ancestors_when_a_subtask_is_added_under_them(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let deepest_task_id = add_task_chain(&db_pool, &signed_in_person, &task_group_id, 2).await;
    let chain_tasks = response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/task-groups/{task_group_id}/tasks"),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&chain_tasks, "Level 1"),
        true,
    )
    .await;

    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "New subtask",
        Some(&deepest_task_id),
    )
    .await;

    for task_name in ["Level 1", "Level 2", "New subtask"] {
        assert_eq!(
            find_task_named(&group_tasks, task_name)["isDone"],
            false,
            "{task_name} should be open"
        );
    }
    assert_eq!(group_tasks["openCount"], 3);
}

#[sqlx::test]
async fn refuses_a_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;

    for rejected_name in [" ", &"a".repeat(501)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/task-groups/{task_group_id}/tasks"),
            json!({ "name": rejected_name }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a task name is 1 to 500 characters" })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_group_or_parent(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group_id = create_group_id(&db_pool, &owner).await;
    let owner_tasks = add_task(&db_pool, &owner, &owner_group_id, "Mine", None).await;
    let owner_task_id = task_id_named(&owner_tasks, "Mine");
    let other_person_group_id = create_group_id(&db_pool, &other_person).await;

    let foreign_group_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/task-groups/{owner_group_id}/tasks"),
        json!({ "name": "Sneaky" }),
        &other_person.session_cookies,
    )
    .await;
    let foreign_parent_response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/task-groups/{other_person_group_id}/tasks"),
        json!({ "name": "Sneaky", "parentId": owner_task_id }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(foreign_group_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_parent_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn adding_a_task_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;

    let response = post_json(
        build_test_app(db_pool),
        &format!("/task-groups/{task_group_id}/tasks"),
        json!({ "name": "Nope" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
