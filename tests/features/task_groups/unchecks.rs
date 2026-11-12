use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::read_card;
use crate::card_schedules::reads::read_board;
use crate::cards::create::{card, card_titles_by_list};
use crate::cards::history::{move_shapes, read_history};
use crate::cards::triggers::{import_linked_tasks, release_board, save_triggers};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, find_task_named, task_id_named};

fn every_day() -> serde_json::Value {
    json!({ "frequency": "daily", "interval": 1, "ends": { "never": true } })
}

async fn uncheck_every_midnight(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    group_tasks: &serde_json::Value,
) {
    let group = &group_tasks["group"];
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/task-groups/{}", group["id"].as_str().expect("an id")),
        json!({
            "name": group["name"],
            "unchecks": { "repeat": every_day(), "time": "00:00" }
        }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn tick_before_the_last_uncheck(db_pool: &PgPool, task_id: &str) {
    sqlx::query(
        "UPDATE tasks SET completed_at = now() - interval '2 days' WHERE id = $1::text::uuid",
    )
    .bind(task_id)
    .execute(db_pool)
    .await
    .expect("moving the tick back should run");
}

async fn read_group_tasks(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    group_tasks: &serde_json::Value,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "/task-groups/{}/tasks",
            group_tasks["group"]["id"].as_str().expect("an id")
        ),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn open_count_of_group(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> i64 {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/task-groups",
        &signed_in_person.session_cookies,
    )
    .await;
    response_json(response).await[0]["openTaskCount"]
        .as_i64()
        .expect("an open count")
}

struct DoneReadings {
    task_is_done: bool,
    group_open_count: i64,
    card_linked_task_is_done: bool,
    card_done_count: i64,
    board_done_count: i64,
}

async fn read_done_everywhere(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    group_tasks: &serde_json::Value,
    board: &serde_json::Value,
    card_id: &str,
) -> DoneReadings {
    let card_detail = read_card(db_pool, signed_in_person, card_id).await;
    let read_board_json = read_board(db_pool, signed_in_person, board).await;
    DoneReadings {
        task_is_done: find_task_named(
            &read_group_tasks(db_pool, signed_in_person, group_tasks).await,
            "Draft",
        )["isDone"]
            .as_bool()
            .expect("isDone"),
        group_open_count: open_count_of_group(db_pool, signed_in_person).await,
        card_linked_task_is_done: card_detail["linkedTasks"][0]["isDone"]
            .as_bool()
            .expect("isDone"),
        card_done_count: card_detail["card"]["doneLinkedTaskCount"]
            .as_i64()
            .expect("a done count"),
        board_done_count: card(&read_board_json, "Release")["doneLinkedTaskCount"]
            .as_i64()
            .expect("a done count"),
    }
}

#[sqlx::test]
async fn a_tick_from_before_the_uncheck_reads_open_on_every_screen(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &signed_in_person).await;
    let group_tasks = import_linked_tasks(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        &["Draft"],
    )
    .await;
    uncheck_every_midnight(&db_pool, &signed_in_person, &group_tasks).await;
    let draft_task_id = task_id_named(&group_tasks, "Draft");
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;

    let fresh_readings = read_done_everywhere(
        &db_pool,
        &signed_in_person,
        &group_tasks,
        &release_board.board,
        &release_board.card_id,
    )
    .await;
    tick_before_the_last_uncheck(&db_pool, &draft_task_id).await;
    let stale_readings = read_done_everywhere(
        &db_pool,
        &signed_in_person,
        &group_tasks,
        &release_board.board,
        &release_board.card_id,
    )
    .await;

    assert!(fresh_readings.task_is_done);
    assert_eq!(fresh_readings.group_open_count, 1);
    assert!(fresh_readings.card_linked_task_is_done);
    assert_eq!(fresh_readings.card_done_count, 1);
    assert_eq!(fresh_readings.board_done_count, 1);
    assert!(!stale_readings.task_is_done);
    assert_eq!(stale_readings.group_open_count, 2);
    assert!(!stale_readings.card_linked_task_is_done);
    assert_eq!(stale_readings.card_done_count, 0);
    assert_eq!(stale_readings.board_done_count, 0);
}

#[sqlx::test]
async fn ticking_a_stale_last_linked_task_refreshes_it_and_finishes_the_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &signed_in_person).await;
    save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({ "finishedListId": release_board.done_list_id }),
    )
    .await;
    let group_tasks = import_linked_tasks(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        &["Draft", "Review"],
    )
    .await;
    uncheck_every_midnight(&db_pool, &signed_in_person, &group_tasks).await;
    let draft_task_id = task_id_named(&group_tasks, "Draft");
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;
    tick_before_the_last_uncheck(&db_pool, &draft_task_id).await;

    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Review"),
        true,
    )
    .await;
    let titles_while_draft_is_stale = card_titles_by_list(
        &read_board(&db_pool, &signed_in_person, &release_board.board).await,
        "Done",
    );
    let ticked_group = set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;

    assert!(titles_while_draft_is_stale.is_empty());
    assert_eq!(find_task_named(&ticked_group, "Draft")["isDone"], true);
    assert_eq!(
        card_titles_by_list(
            &read_board(&db_pool, &signed_in_person, &release_board.board).await,
            "Done"
        ),
        vec!["Release"]
    );
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_board.card_id).await),
        vec![json!({ "from": "To Do", "to": "Done", "reason": "finished", "taskTitle": "Draft" })]
    );
}

#[sqlx::test]
async fn ticking_a_parent_refreshes_its_stale_subtasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let daily_group = create_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Daily", "unchecks": { "repeat": every_day(), "time": "00:00" } }),
    )
    .await;
    let daily_group_id = daily_group["id"].as_str().expect("an id");
    let parent_tasks = add_task(&db_pool, &signed_in_person, daily_group_id, "Workout", None).await;
    let parent_task_id = task_id_named(&parent_tasks, "Workout");
    let child_tasks = add_task(
        &db_pool,
        &signed_in_person,
        daily_group_id,
        "Stretch",
        Some(&parent_task_id),
    )
    .await;
    let child_task_id = task_id_named(&child_tasks, "Stretch");
    set_task_done(&db_pool, &signed_in_person, &child_task_id, true).await;
    tick_before_the_last_uncheck(&db_pool, &child_task_id).await;

    let ticked_group = set_task_done(&db_pool, &signed_in_person, &parent_task_id, true).await;

    assert_eq!(find_task_named(&ticked_group, "Workout")["isDone"], true);
    assert_eq!(find_task_named(&ticked_group, "Stretch")["isDone"], true);
    assert_eq!(ticked_group["openCount"], 0);
}
