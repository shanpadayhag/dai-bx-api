use std::collections::HashMap;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, find_task_named, task_id_named};

pub async fn set_task_done(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
    is_done: bool,
) -> serde_json::Value {
    let completion_action = if is_done { "complete" } else { "reopen" };
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{task_id}/{completion_action}"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn is_done(group_tasks: &serde_json::Value, task_name: &str) -> bool {
    find_task_named(group_tasks, task_name)["isDone"]
        .as_bool()
        .expect("isDone should be a boolean")
}

fn assert_no_done_task_hides_an_open_one(group_tasks: &serde_json::Value) {
    let task_list = group_tasks["tasks"].as_array().expect("a list");
    let is_done_by_id: HashMap<&str, bool> = task_list
        .iter()
        .map(|task_json| {
            (
                task_json["id"].as_str().expect("an id"),
                task_json["isDone"].as_bool().expect("a boolean"),
            )
        })
        .collect();
    let parent_id_by_id: HashMap<&str, Option<&str>> = task_list
        .iter()
        .map(|task_json| {
            (
                task_json["id"].as_str().expect("an id"),
                task_json["parentId"].as_str(),
            )
        })
        .collect();
    for (task_id, task_is_done) in &is_done_by_id {
        if *task_is_done {
            continue;
        }
        let mut ancestor_id = parent_id_by_id[task_id];
        while let Some(current_ancestor_id) = ancestor_id {
            assert!(
                !is_done_by_id[current_ancestor_id],
                "open task {task_id} sits under done task {current_ancestor_id}"
            );
            ancestor_id = parent_id_by_id[current_ancestor_id];
        }
    }
}

async fn seed_family(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> serde_json::Value {
    let task_group_id = create_group_id(db_pool, signed_in_person).await;
    let parent_tasks = add_task(db_pool, signed_in_person, &task_group_id, "Parent", None).await;
    let parent_task_id = task_id_named(&parent_tasks, "Parent");
    let child_tasks = add_task(
        db_pool,
        signed_in_person,
        &task_group_id,
        "Child",
        Some(&parent_task_id),
    )
    .await;
    let child_task_id = task_id_named(&child_tasks, "Child");
    add_task(
        db_pool,
        signed_in_person,
        &task_group_id,
        "Second child",
        Some(&parent_task_id),
    )
    .await;
    add_task(
        db_pool,
        signed_in_person,
        &task_group_id,
        "Grandchild",
        Some(&child_task_id),
    )
    .await;
    add_task(db_pool, signed_in_person, &task_group_id, "Neighbour", None).await
}

#[sqlx::test]
async fn completing_a_parent_completes_every_open_task_beneath_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let family_tasks = seed_family(&db_pool, &signed_in_person).await;

    let group_tasks = set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&family_tasks, "Parent"),
        true,
    )
    .await;

    for task_name in ["Parent", "Child", "Second child", "Grandchild"] {
        assert!(
            is_done(&group_tasks, task_name),
            "{task_name} should be done"
        );
    }
    assert!(!is_done(&group_tasks, "Neighbour"));
    assert_eq!(group_tasks["openCount"], 1);
    assert_eq!(group_tasks["doneCount"], 4);
    assert_no_done_task_hides_an_open_one(&group_tasks);
}

#[sqlx::test]
async fn reopening_a_deep_subtask_reopens_its_done_ancestors_but_no_descendants(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let family_tasks = seed_family(&db_pool, &signed_in_person).await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&family_tasks, "Parent"),
        true,
    )
    .await;

    let group_tasks = set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&family_tasks, "Child"),
        false,
    )
    .await;

    assert!(!is_done(&group_tasks, "Parent"));
    assert!(!is_done(&group_tasks, "Child"));
    assert!(is_done(&group_tasks, "Grandchild"));
    assert!(is_done(&group_tasks, "Second child"));
    assert_no_done_task_hides_an_open_one(&group_tasks);
}

#[sqlx::test]
async fn completing_the_last_open_child_leaves_the_parent_open(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let family_tasks = seed_family(&db_pool, &signed_in_person).await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&family_tasks, "Child"),
        true,
    )
    .await;

    let group_tasks = set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&family_tasks, "Second child"),
        true,
    )
    .await;

    assert!(is_done(&group_tasks, "Child"));
    assert!(is_done(&group_tasks, "Second child"));
    assert!(!is_done(&group_tasks, "Parent"));
    assert_no_done_task_hides_an_open_one(&group_tasks);
}

#[sqlx::test]
async fn completing_or_reopening_twice_changes_nothing(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let family_tasks = seed_family(&db_pool, &signed_in_person).await;
    let neighbour_task_id = task_id_named(&family_tasks, "Neighbour");

    set_task_done(&db_pool, &signed_in_person, &neighbour_task_id, true).await;
    let twice_completed =
        set_task_done(&db_pool, &signed_in_person, &neighbour_task_id, true).await;
    set_task_done(&db_pool, &signed_in_person, &neighbour_task_id, false).await;
    let twice_reopened =
        set_task_done(&db_pool, &signed_in_person, &neighbour_task_id, false).await;

    assert!(is_done(&twice_completed, "Neighbour"));
    assert!(!is_done(&twice_reopened, "Neighbour"));
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_malformed_task(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_tasks = seed_family(&db_pool, &owner).await;
    let owner_task_id = task_id_named(&owner_tasks, "Parent");

    for completion_path in [
        format!("/tasks/{owner_task_id}/complete"),
        format!("/tasks/{owner_task_id}/reopen"),
        String::from("/tasks/not-a-uuid/complete"),
    ] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &completion_path,
            json!({}),
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{completion_path}"
        );
    }
}

#[sqlx::test]
async fn completing_a_task_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let family_tasks = seed_family(&db_pool, &signed_in_person).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{}/complete", task_id_named(&family_tasks, "Parent")),
        json!({}),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
