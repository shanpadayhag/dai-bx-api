use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::activity::feed::{entry_kinds, list_entries};
use crate::support::{build_test_app, post_json_with_cookies, sign_up_with_authenticator};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};
use crate::tasks::import::import_group;
use crate::tracking::start::add_top_level_task;

#[sqlx::test]
async fn ticking_a_task_adds_a_task_completed_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;

    set_task_done(&db_pool, &signed_in_person, &task_id, true).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["task_completed"]);
    assert_eq!(activity_entries[0]["title"], "Write the changelog");
    assert_eq!(activity_entries[0]["contextName"], "Today");
    assert_eq!(
        activity_entries[0]["details"],
        json!({ "isSubtask": false })
    );
}

#[sqlx::test]
async fn ticking_and_reopening_a_subtask_marks_both_entries_as_a_subtask(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) = add_top_level_task(&db_pool, &signed_in_person, "Release").await;
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Tag the build",
        Some(&task_id),
    )
    .await;
    let subtask_id = task_id_named(&group_tasks, "Tag the build");

    set_task_done(&db_pool, &signed_in_person, &subtask_id, true).await;
    set_task_done(&db_pool, &signed_in_person, &subtask_id, false).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(
        entry_kinds(&activity_entries),
        vec!["task_reopened", "task_completed"]
    );
    for activity_entry in &activity_entries {
        assert_eq!(activity_entry["title"], "Tag the build");
        assert_eq!(activity_entry["details"], json!({ "isSubtask": true }));
    }
}

#[sqlx::test]
async fn ticking_an_unknown_task_adds_no_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/tasks/00000000-0000-0000-0000-000000000000/complete",
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(list_entries(&db_pool, &signed_in_person).await.is_empty());
}

#[sqlx::test]
async fn importing_a_group_adds_no_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = import_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Today",
            "generatedBy": "daily plan",
            "tasks": [{ "name": "Write the changelog", "tasks": [] }],
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(list_entries(&db_pool, &signed_in_person).await.is_empty());
}
