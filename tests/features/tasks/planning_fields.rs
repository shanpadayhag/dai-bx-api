use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, find_task_named, task_id_named};
use crate::tasks::moving::move_task;

async fn patch_task(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
    task_changes: Value,
) -> axum::http::Response<axum::body::Body> {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{task_id}"),
        task_changes,
        &signed_in_person.session_cookies,
    )
    .await
}

fn all_planning_fields() -> Value {
    json!({
        "priority": "high",
        "due": { "date": "2026-11-21", "time": "17:00" },
        "planBetween": { "start": "22:00", "end": "02:00" },
    })
}

#[sqlx::test]
async fn a_new_task_is_normal_with_no_due_date_or_window(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;

    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Plan", None).await;

    let new_task = find_task_named(&group_tasks, "Plan");
    assert_eq!(new_task["priority"], "normal");
    assert_eq!(new_task["due"], Value::Null);
    assert_eq!(new_task["planBetween"], Value::Null);
}

#[sqlx::test]
async fn sets_changes_and_clears_each_planning_field(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Plan", None).await;
    let task_id = task_id_named(&group_tasks, "Plan");

    let set_tasks = response_json(
        patch_task(&db_pool, &signed_in_person, &task_id, all_planning_fields()).await,
    )
    .await;
    let changed_tasks = response_json(
        patch_task(
            &db_pool,
            &signed_in_person,
            &task_id,
            json!({ "priority": "low", "due": { "date": "2026-11-22" } }),
        )
        .await,
    )
    .await;
    let cleared_tasks = response_json(
        patch_task(
            &db_pool,
            &signed_in_person,
            &task_id,
            json!({ "priority": "normal", "due": null, "planBetween": null }),
        )
        .await,
    )
    .await;

    let set_task = find_task_named(&set_tasks, "Plan");
    assert_eq!(set_task["priority"], "high");
    assert_eq!(
        set_task["due"],
        json!({ "date": "2026-11-21", "time": "17:00" })
    );
    assert_eq!(
        set_task["planBetween"],
        json!({ "start": "22:00", "end": "02:00" })
    );
    let changed_task = find_task_named(&changed_tasks, "Plan");
    assert_eq!(changed_task["priority"], "low");
    assert_eq!(
        changed_task["due"],
        json!({ "date": "2026-11-22", "time": null })
    );
    assert_eq!(
        changed_task["planBetween"],
        json!({ "start": "22:00", "end": "02:00" })
    );
    let cleared_task = find_task_named(&cleared_tasks, "Plan");
    assert_eq!(cleared_task["priority"], "normal");
    assert_eq!(cleared_task["due"], Value::Null);
    assert_eq!(cleared_task["planBetween"], Value::Null);
}

#[sqlx::test]
async fn refuses_bad_planning_fields(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Plan", None).await;
    let task_id = task_id_named(&group_tasks, "Plan");

    for (rejected_changes, expected_error) in [
        (
            json!({ "priority": "urgent" }),
            "a priority is high, normal or low",
        ),
        (
            json!({ "due": { "date": "21/11/2026" } }),
            "a date is written as YYYY-MM-DD",
        ),
        (
            json!({ "due": { "date": "2026-11-21", "time": "5pm" } }),
            "a time is written as HH:MM",
        ),
        (
            json!({ "planBetween": { "start": "08:00", "end": "08:00" } }),
            "a window starts and ends at different times",
        ),
    ] {
        let response = patch_task(&db_pool, &signed_in_person, &task_id, rejected_changes).await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response_json(response).await["error"], expected_error);
    }
}

#[sqlx::test]
async fn refuses_planning_fields_on_a_subtask(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Plan", None).await;
    let parent_task_id = task_id_named(&group_tasks, "Plan");
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Outline",
        Some(&parent_task_id),
    )
    .await;
    let subtask_id = task_id_named(&group_tasks, "Outline");

    for rejected_changes in [
        json!({ "priority": "high" }),
        json!({ "due": { "date": "2026-11-21" } }),
        json!({ "planBetween": { "start": "08:00", "end": "10:00" } }),
    ] {
        let response = patch_task(&db_pool, &signed_in_person, &subtask_id, rejected_changes).await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await["error"],
            "only a top-level task has a priority, a due date or a window"
        );
    }
}

#[sqlx::test]
async fn moving_a_task_under_another_clears_its_planning_fields(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    add_task(&db_pool, &signed_in_person, &task_group_id, "Parent", None).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Plan", None).await;
    let parent_task_id = task_id_named(&group_tasks, "Parent");
    let task_id = task_id_named(&group_tasks, "Plan");
    patch_task(&db_pool, &signed_in_person, &task_id, all_planning_fields()).await;

    let moved_under = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id,
            json!({ "groupId": task_group_id, "parentId": parent_task_id, "position": 0 }),
        )
        .await,
    )
    .await;
    let moved_back = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id,
            json!({ "groupId": task_group_id, "parentId": null, "position": 0 }),
        )
        .await,
    )
    .await;

    for moved_tasks in [moved_under, moved_back] {
        let moved_task = find_task_named(&moved_tasks, "Plan");
        assert_eq!(moved_task["priority"], "normal");
        assert_eq!(moved_task["due"], Value::Null);
        assert_eq!(moved_task["planBetween"], Value::Null);
    }
}
