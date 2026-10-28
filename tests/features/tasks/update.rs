use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, find_task_named, task_id_named};

#[sqlx::test]
async fn renames_a_task_and_sets_its_estimate(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Draft", None).await;
    let draft_task_id = task_id_named(&group_tasks, "Draft");

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{draft_task_id}"),
        json!({ "name": "  Write the migration ", "estimateMinutes": 90 }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_tasks = response_json(response).await;
    let updated_task = find_task_named(&updated_tasks, "Write the migration");
    assert_eq!(updated_task["id"], draft_task_id);
    assert_eq!(updated_task["estimateMinutes"], 90);
    assert_eq!(updated_tasks["group"]["id"], task_group_id);
}

#[sqlx::test]
async fn clears_the_estimate_with_null_and_keeps_it_when_absent(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Task", None).await;
    let task_path = format!("/tasks/{}", task_id_named(&group_tasks, "Task"));
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &task_path,
        json!({ "estimateMinutes": 45 }),
        &signed_in_person.session_cookies,
    )
    .await;

    let renamed_tasks = response_json(
        patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &task_path,
            json!({ "name": "Renamed" }),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    let cleared_tasks = response_json(
        patch_json_with_cookies(
            build_test_app(db_pool),
            &task_path,
            json!({ "estimateMinutes": null }),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(
        find_task_named(&renamed_tasks, "Renamed")["estimateMinutes"],
        45
    );
    assert_eq!(
        find_task_named(&cleared_tasks, "Renamed")["estimateMinutes"],
        serde_json::Value::Null
    );
}

#[sqlx::test]
async fn refuses_a_bad_name_or_an_estimate_outside_one_minute_to_a_day(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Task", None).await;
    let task_path = format!("/tasks/{}", task_id_named(&group_tasks, "Task"));

    for (rejected_changes, expected_error) in [
        (json!({ "name": "" }), "a task name is 1 to 500 characters"),
        (
            json!({ "estimateMinutes": 0 }),
            "an estimate is 1 minute to 24 hours",
        ),
        (
            json!({ "estimateMinutes": 1441 }),
            "an estimate is 1 minute to 24 hours",
        ),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &task_path,
            rejected_changes,
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": expected_error })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_malformed_task(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group_id = create_group_id(&db_pool, &owner).await;
    let owner_tasks = add_task(&db_pool, &owner, &owner_group_id, "Mine", None).await;

    for task_path in [
        format!("/tasks/{}", task_id_named(&owner_tasks, "Mine")),
        String::from("/tasks/not-a-uuid"),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &task_path,
            json!({ "name": "Taken over" }),
            &other_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn updating_a_task_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Task", None).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{}", task_id_named(&group_tasks, "Task")),
        json!({ "name": "Nope" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
