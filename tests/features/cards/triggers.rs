use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::board_lists::create::{board_with_lists, list_id};
use crate::calendar_events::create::event_id;
use crate::card_schedules::create::read_card;
use crate::card_schedules::reads::read_board;
use crate::cards::create::{board_with_cards, card_id, card_titles_by_list};
use crate::cards::history::{drag_card, move_shapes, read_history, shape};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, put_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};
use crate::tasks::import::import_group;
use crate::tracking::finish::post_finish;
use crate::tracking::start::{start_card, start_task};

pub async fn put_triggers(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    put_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/triggers"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn save_triggers(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = put_triggers(db_pool, signed_in_person, card_id, request_body).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub struct ReleaseBoard {
    pub board: serde_json::Value,
    pub card_id: String,
    pub doing_list_id: String,
    pub done_list_id: String,
}

pub async fn release_board(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> ReleaseBoard {
    let board = board_with_cards(
        db_pool,
        signed_in_person,
        &[
            ("To Do", &["Release"]),
            ("Doing", &["Other"]),
            ("Done", &[]),
        ],
    )
    .await;
    ReleaseBoard {
        card_id: card_id(&board, "Release").to_owned(),
        doing_list_id: list_id(&board, "Doing").to_owned(),
        done_list_id: list_id(&board, "Done").to_owned(),
        board,
    }
}

pub async fn import_linked_tasks(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    task_names: &[&str],
) -> serde_json::Value {
    let mut imported_tasks: Vec<serde_json::Value> = task_names
        .iter()
        .map(|task_name| json!({ "name": task_name, "card": card_id, "tasks": [] }))
        .collect();
    imported_tasks.push(json!({ "name": "Inbox zero", "tasks": [] }));
    let response = import_group(
        db_pool,
        signed_in_person,
        json!({ "name": "Today", "tasks": imported_tasks }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn finish_now(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    running_item: &serde_json::Value,
    marks_done: bool,
) {
    let response = post_finish(
        db_pool,
        signed_in_person,
        event_id(running_item),
        json!({ "endsAt": UtcInstant::now().to_iso_text(), "markDone": marks_done }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn titles_in(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    release_board: &ReleaseBoard,
    list_name: &str,
) -> Vec<String> {
    card_titles_by_list(
        &read_board(db_pool, signed_in_person, &release_board.board).await,
        list_name,
    )
}

fn target(list_id: &str, list_name: &str) -> serde_json::Value {
    json!({ "listId": list_id, "listName": list_name })
}

#[sqlx::test]
async fn saves_both_one_or_neither_trigger(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &signed_in_person).await;

    let both_saved = save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({
            "startedListId": release_board.doing_list_id,
            "finishedListId": release_board.done_list_id,
        }),
    )
    .await;
    let one_saved = save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({ "startedListId": release_board.doing_list_id, "finishedListId": null }),
    )
    .await;
    let card_with_one = read_card(&db_pool, &signed_in_person, &release_board.card_id).await;
    let none_saved = save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({}),
    )
    .await;

    assert_eq!(
        both_saved,
        json!({ "triggers": {
            "started": target(&release_board.doing_list_id, "Doing"),
            "finished": target(&release_board.done_list_id, "Done"),
        }})
    );
    let one_trigger = json!({
        "started": target(&release_board.doing_list_id, "Doing"),
        "finished": null,
    });
    assert_eq!(one_saved, json!({ "triggers": one_trigger }));
    assert_eq!(card_with_one["triggers"], one_trigger);
    assert_eq!(
        none_saved,
        json!({ "triggers": { "started": null, "finished": null } })
    );
}

#[sqlx::test]
async fn answers_not_found_for_a_column_on_another_board_or_someone_else_s_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &owner).await;
    let other_board = board_with_lists(&db_pool, &owner, &["Elsewhere"]).await;

    let other_board_response = put_triggers(
        &db_pool,
        &owner,
        &release_board.card_id,
        json!({ "startedListId": list_id(&other_board, "Elsewhere") }),
    )
    .await;
    let foreign_response = put_triggers(
        &db_pool,
        &other_person,
        &release_board.card_id,
        json!({ "startedListId": release_board.doing_list_id }),
    )
    .await;

    assert_eq!(other_board_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn every_start_moves_the_card_to_the_top_of_its_started_column(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let release_board = release_board(&db_pool, &signed_in_person).await;
    let to_do_list_id = list_id(&release_board.board, "To Do").to_owned();
    save_triggers(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        json!({ "startedListId": release_board.doing_list_id }),
    )
    .await;
    let group_tasks = import_linked_tasks(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        &["Ship"],
    )
    .await;

    let task_run = start_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Ship"),
    )
    .await;
    let titles_after_task_start =
        titles_in(&db_pool, &signed_in_person, &release_board, "Doing").await;
    finish_now(&db_pool, &signed_in_person, &task_run, false).await;
    drag_card(
        &db_pool,
        &signed_in_person,
        &release_board.card_id,
        &to_do_list_id,
        0,
    )
    .await;
    let first_card_run = start_card(&db_pool, &signed_in_person, &release_board.card_id).await;
    finish_now(&db_pool, &signed_in_person, &first_card_run, false).await;
    start_card(&db_pool, &signed_in_person, &release_board.card_id).await;

    assert_eq!(titles_after_task_start, vec!["Release", "Other"]);
    assert_eq!(
        titles_in(&db_pool, &signed_in_person, &release_board, "Doing").await,
        vec!["Release", "Other"]
    );
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_board.card_id).await),
        vec![
            shape("To Do", "Doing", "started"),
            shape("Doing", "To Do", "byHand"),
            json!({ "from": "To Do", "to": "Doing", "reason": "started", "taskTitle": "Ship" }),
        ]
    );
}

#[sqlx::test]
async fn completing_the_last_open_linked_task_moves_the_card_to_its_finished_column(
    db_pool: PgPool,
) {
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
    let draft_task_id = task_id_named(&group_tasks, "Draft");

    set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, false).await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Inbox zero"),
        true,
    )
    .await;
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;
    let titles_before_last = titles_in(&db_pool, &signed_in_person, &release_board, "To Do").await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Review"),
        true,
    )
    .await;

    assert_eq!(titles_before_last, vec!["Release"]);
    assert_eq!(
        titles_in(&db_pool, &signed_in_person, &release_board, "Done").await,
        vec!["Release"]
    );
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_board.card_id).await),
        vec![json!({ "from": "To Do", "to": "Done", "reason": "finished", "taskTitle": "Review" })]
    );
}

#[sqlx::test]
async fn marking_the_last_task_done_from_finish_moves_the_card_too(db_pool: PgPool) {
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
        &["Ship"],
    )
    .await;

    let task_run = start_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Ship"),
    )
    .await;
    finish_now(&db_pool, &signed_in_person, &task_run, true).await;

    assert_eq!(
        titles_in(&db_pool, &signed_in_person, &release_board, "Done").await,
        vec!["Release"]
    );
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_board.card_id).await),
        vec![json!({ "from": "To Do", "to": "Done", "reason": "finished", "taskTitle": "Ship" })]
    );
}

#[sqlx::test]
async fn a_top_level_task_s_open_subtasks_on_the_same_card_count_as_completed(db_pool: PgPool) {
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
        &["Ship"],
    )
    .await;
    let ship_task_id = task_id_named(&group_tasks, "Ship");
    let group_id = group_tasks["group"]["id"].as_str().expect("a group id");
    let with_subtask = add_task(
        &db_pool,
        &signed_in_person,
        group_id,
        "Tag the build",
        Some(&ship_task_id),
    )
    .await;
    let subtask_id =
        Uuid::parse_str(&task_id_named(&with_subtask, "Tag the build")).expect("a task id");
    sqlx::query("UPDATE tasks SET card_id = $1 WHERE id = $2")
        .bind(Uuid::parse_str(&release_board.card_id).expect("a card id"))
        .bind(subtask_id)
        .execute(&db_pool)
        .await
        .expect("the subtask should link");

    set_task_done(&db_pool, &signed_in_person, &ship_task_id, true).await;

    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_board.card_id).await),
        vec![json!({ "from": "To Do", "to": "Done", "reason": "finished", "taskTitle": "Ship" })]
    );
}

#[sqlx::test]
async fn a_deleted_target_column_flags_the_trigger_and_moves_nothing(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["Release"]), ("Doing", &[]), ("Done", &[])],
    )
    .await;
    let release_card_id = card_id(&board, "Release").to_owned();
    save_triggers(
        &db_pool,
        &signed_in_person,
        &release_card_id,
        json!({
            "startedListId": list_id(&board, "Doing"),
            "finishedListId": list_id(&board, "Done"),
        }),
    )
    .await;
    for deleted_list_name in ["Doing", "Done"] {
        let response = delete_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/lists/{}", list_id(&board, deleted_list_name)),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let group_tasks =
        import_linked_tasks(&db_pool, &signed_in_person, &release_card_id, &["Ship"]).await;

    let task_run = start_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Ship"),
    )
    .await;
    finish_now(&db_pool, &signed_in_person, &task_run, true).await;

    let flagged_target = json!({ "listId": null, "listName": null });
    assert_eq!(
        read_card(&db_pool, &signed_in_person, &release_card_id).await["triggers"],
        json!({ "started": flagged_target, "finished": flagged_target })
    );
    assert_eq!(
        read_history(&db_pool, &signed_in_person, &release_card_id).await["moves"],
        json!([])
    );
}
