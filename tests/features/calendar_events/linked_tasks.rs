use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id, post_event};
use crate::calendar_events::list::list_events;
use crate::calendar_events::update::patch_event;
use crate::card_schedules::create::{board_with_review_card, create_schedule, merged, once_at};
use crate::card_schedules::reads::calendar_week;
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::task_groups::unchecks::{tick_before_the_last_uncheck, uncheck_every_midnight};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, create_group_id, task_id_named};

const WEEK_QUERY: &str = "start=2026-11-02&days=7&timeZone=UTC";

struct LinkableTasks {
    group_tasks: Value,
    top_level_task_id: String,
    subtask_id: String,
}

async fn linkable_tasks(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> LinkableTasks {
    let task_group_id = create_group_id(db_pool, signed_in_person).await;
    let group_tasks = add_task(
        db_pool,
        signed_in_person,
        &task_group_id,
        "Write the plan",
        None,
    )
    .await;
    let top_level_task_id = task_id_named(&group_tasks, "Write the plan");
    let group_tasks = add_task(
        db_pool,
        signed_in_person,
        &task_group_id,
        "Outline",
        Some(&top_level_task_id),
    )
    .await;
    LinkableTasks {
        subtask_id: task_id_named(&group_tasks, "Outline"),
        top_level_task_id,
        group_tasks,
    }
}

async fn planned_block(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> String {
    event_id(
        &create_planned_event(
            db_pool,
            signed_in_person,
            "Focus",
            "2026-11-05T09:00",
            "2026-11-05T10:00",
        )
        .await,
    )
    .to_owned()
}

async fn listed_event(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> Value {
    list_events(db_pool, signed_in_person, WEEK_QUERY)
        .await
        .into_iter()
        .next()
        .expect("one listed event")
}

#[sqlx::test]
async fn creates_a_planned_event_linked_to_a_task(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;

    let response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Write the plan",
            "start": "2026-11-05T09:00",
            "end": "2026-11-05T10:00",
            "inputZone": "UTC",
            "taskId": linkable_tasks.top_level_task_id,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created_event = response_json(response).await;
    assert_eq!(
        created_event["task"],
        json!({
            "id": linkable_tasks.top_level_task_id,
            "name": "Write the plan",
            "groupId": linkable_tasks.group_tasks["group"]["id"],
            "groupName": "Today",
            "done": false,
        })
    );
    assert_eq!(created_event["isScheduleBlock"], false);
    assert_eq!(
        listed_event(&db_pool, &signed_in_person).await["task"],
        created_event["task"]
    );
}

#[sqlx::test]
async fn links_changes_and_clears_a_task_on_a_planned_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let planned_event_id = planned_block(&db_pool, &signed_in_person).await;

    let linked_event = response_json(
        patch_event(
            &db_pool,
            &signed_in_person,
            &planned_event_id,
            json!({ "taskId": linkable_tasks.top_level_task_id }),
        )
        .await,
    )
    .await;
    let renamed_event = response_json(
        patch_event(
            &db_pool,
            &signed_in_person,
            &planned_event_id,
            json!({ "title": "Deep work" }),
        )
        .await,
    )
    .await;
    let cleared_event = response_json(
        patch_event(
            &db_pool,
            &signed_in_person,
            &planned_event_id,
            json!({ "taskId": null }),
        )
        .await,
    )
    .await;

    assert_eq!(
        linked_event["task"]["id"],
        linkable_tasks.top_level_task_id.as_str()
    );
    assert_eq!(
        renamed_event["task"]["id"],
        linkable_tasks.top_level_task_id.as_str()
    );
    assert_eq!(renamed_event["title"], "Deep work");
    assert_eq!(cleared_event["task"], Value::Null);
}

#[sqlx::test]
async fn refuses_a_subtask_and_another_person_s_task(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let other_tasks = linkable_tasks(&db_pool, &other_person).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let planned_event_id = planned_block(&db_pool, &signed_in_person).await;

    let subtask_response = patch_event(
        &db_pool,
        &signed_in_person,
        &planned_event_id,
        json!({ "taskId": linkable_tasks.subtask_id }),
    )
    .await;
    let foreign_response = patch_event(
        &db_pool,
        &signed_in_person,
        &planned_event_id,
        json!({ "taskId": other_tasks.top_level_task_id }),
    )
    .await;

    assert_eq!(subtask_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(subtask_response).await["error"],
        "only a top-level task can be linked to a planned event"
    );
    assert_eq!(foreign_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn refuses_a_task_on_a_repeating_event_or_a_schedule_block(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let series_response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Gym",
            "start": "2026-11-05T09:00",
            "end": "2026-11-05T10:00",
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
            "taskId": linkable_tasks.top_level_task_id,
        }),
    )
    .await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            once_at(&scheduled_card.to_do_list_id, "2030-01-08T08:00"),
            json!({ "block": { "minutes": 60 } }),
        ),
    )
    .await;
    let block_event =
        calendar_week(&db_pool, &signed_in_person, "2030-01-07").await["events"][0].clone();

    let block_response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&block_event),
        json!({ "taskId": linkable_tasks.top_level_task_id }),
    )
    .await;

    assert_eq!(series_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(block_event["isScheduleBlock"], true);
    assert_eq!(block_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(block_response).await["error"],
        "a repeating event or a schedule block can't be linked to a task"
    );
}

#[sqlx::test]
async fn is_done_follows_the_task_and_its_group_s_uncheck(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let planned_event_id = planned_block(&db_pool, &signed_in_person).await;
    patch_event(
        &db_pool,
        &signed_in_person,
        &planned_event_id,
        json!({ "taskId": linkable_tasks.top_level_task_id }),
    )
    .await;

    let before_tick = listed_event(&db_pool, &signed_in_person).await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &linkable_tasks.top_level_task_id,
        true,
    )
    .await;
    let after_tick = listed_event(&db_pool, &signed_in_person).await;
    uncheck_every_midnight(&db_pool, &signed_in_person, &linkable_tasks.group_tasks).await;
    tick_before_the_last_uncheck(&db_pool, &linkable_tasks.top_level_task_id).await;
    let after_uncheck = listed_event(&db_pool, &signed_in_person).await;

    assert_eq!(before_tick["task"]["done"], false);
    assert_eq!(after_tick["task"]["done"], true);
    assert_eq!(after_uncheck["task"]["done"], false);
}

#[sqlx::test]
async fn deleting_the_task_leaves_a_plain_planned_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let planned_event_id = planned_block(&db_pool, &signed_in_person).await;
    patch_event(
        &db_pool,
        &signed_in_person,
        &planned_event_id,
        json!({ "taskId": linkable_tasks.top_level_task_id }),
    )
    .await;

    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{}", linkable_tasks.top_level_task_id),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(delete_response.status(), StatusCode::OK);
    let remaining_event = listed_event(&db_pool, &signed_in_person).await;
    assert_eq!(remaining_event["id"], planned_event_id.as_str());
    assert_eq!(remaining_event["task"], Value::Null);
}

#[sqlx::test]
async fn rename_all_renames_linked_planned_events_too(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let linkable_tasks = linkable_tasks(&db_pool, &signed_in_person).await;
    let planned_event_id = planned_block(&db_pool, &signed_in_person).await;
    patch_event(
        &db_pool,
        &signed_in_person,
        &planned_event_id,
        json!({ "taskId": linkable_tasks.top_level_task_id }),
    )
    .await;
    let rename_path = format!("/tasks/{}", linkable_tasks.top_level_task_id);

    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &rename_path,
        json!({ "name": "Kept title" }),
        &signed_in_person.session_cookies,
    )
    .await;
    let kept_title = listed_event(&db_pool, &signed_in_person).await["title"].clone();
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &rename_path,
        json!({ "name": "Ship the plan", "renameEvents": true }),
        &signed_in_person.session_cookies,
    )
    .await;
    let renamed_event = listed_event(&db_pool, &signed_in_person).await;

    assert_eq!(kept_title, "Focus");
    assert_eq!(renamed_event["title"], "Ship the plan");
    assert_eq!(renamed_event["task"]["name"], "Ship the plan");
}
