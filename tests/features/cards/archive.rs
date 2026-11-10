use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::list_id;
use crate::boards::create::board_id;
use crate::calendar_events::create::event_id;
use crate::card_schedules::create::{
    create_schedule, iso_millis, now_millis, read_card, schedule_id, weekly_review,
};
use crate::card_schedules::pause::pause_schedule;
use crate::card_schedules::reads::read_board;
use crate::cards::create::{add_card, board_with_cards, card_id, card_titles_by_list};
use crate::cards::history::{move_shapes, read_history};
use crate::cards::triggers::{import_linked_tasks, save_triggers};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get_with_cookies, post_json_with_cookies,
    response_json, sign_up_with_authenticator,
};
use crate::tasks::create::task_id_named;
use crate::tracking::start::start_task;

pub async fn post_card_action(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    card_action: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/{card_action}"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn archive_card(db_pool: &PgPool, signed_in_person: &SignedInPerson, card_id: &str) {
    let response = post_card_action(db_pool, signed_in_person, card_id, "archive").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "archived" })
    );
}

async fn restore_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
) -> serde_json::Value {
    let response = post_card_action(db_pool, signed_in_person, card_id, "restore").await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn get_archived_cards(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_id: &str,
    query: &str,
) -> axum::response::Response {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/boards/{board_id}/archived-cards?{query}"),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn list_archived(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_id: &str,
    query: &str,
) -> serde_json::Value {
    let response = get_archived_cards(db_pool, signed_in_person, board_id, query).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn positions_in(board: &serde_json::Value, list_id: &str) -> Vec<i64> {
    let mut positions: Vec<i64> = board["cards"]
        .as_array()
        .expect("a list of cards")
        .iter()
        .filter(|board_card| board_card["listId"] == list_id)
        .map(|board_card| board_card["position"].as_i64().expect("a position"))
        .collect();
    positions.sort_unstable();
    positions
}

fn paused_flags(card_detail: &serde_json::Value) -> Vec<bool> {
    card_detail["schedules"]
        .as_array()
        .expect("a list of schedules")
        .iter()
        .map(|card_schedule| card_schedule["paused"].as_bool().expect("a paused flag"))
        .collect()
}

async fn delete_list(db_pool: &PgPool, signed_in_person: &SignedInPerson, list_id: &str) {
    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{list_id}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn archiving_takes_the_card_off_the_board_and_pauses_only_unpaused_schedules(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["First", "Release", "Last"])],
    )
    .await;
    let to_do_list_id = list_id(&board, "To Do").to_owned();
    let release_card_id = card_id(&board, "Release").to_owned();
    let paused_by_hand = create_schedule(
        &db_pool,
        &signed_in_person,
        &release_card_id,
        weekly_review(&to_do_list_id),
    )
    .await;
    create_schedule(
        &db_pool,
        &signed_in_person,
        &release_card_id,
        weekly_review(&to_do_list_id),
    )
    .await;
    pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&paused_by_hand),
        "pause",
    )
    .await;

    archive_card(&db_pool, &signed_in_person, &release_card_id).await;
    let second_archive =
        post_card_action(&db_pool, &signed_in_person, &release_card_id, "archive").await;
    let archived_board = read_board(&db_pool, &signed_in_person, &board).await;
    let archived_detail = read_card(&db_pool, &signed_in_person, &release_card_id).await;

    assert_eq!(second_archive.status(), StatusCode::CONFLICT);
    assert_eq!(
        card_titles_by_list(&archived_board, "To Do"),
        vec!["First", "Last"]
    );
    assert_eq!(positions_in(&archived_board, &to_do_list_id), vec![0, 1]);
    assert_eq!(paused_flags(&archived_detail), vec![true, true]);
    assert_eq!(archived_detail["archived"]["how"], "byHand");
    assert!(archived_detail["archived"]["at"].is_string());
    assert_eq!(archived_detail["card"]["listId"], json!(null));
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_card_id).await),
        vec![json!({ "from": "To Do", "to": null, "reason": "archivedByHand", "taskTitle": null })]
    );

    let before_restore = now_millis();
    let restored_board = restore_card(&db_pool, &signed_in_person, &release_card_id).await;
    let restored_detail = read_card(&db_pool, &signed_in_person, &release_card_id).await;
    let second_restore =
        post_card_action(&db_pool, &signed_in_person, &release_card_id, "restore").await;

    assert_eq!(second_restore.status(), StatusCode::CONFLICT);
    assert_eq!(
        card_titles_by_list(&restored_board, "To Do"),
        vec!["Release", "First", "Last"]
    );
    assert_eq!(paused_flags(&restored_detail), vec![true, false]);
    let resumed_next_run = iso_millis(
        restored_detail["schedules"][1]["nextRunAt"]
            .as_str()
            .expect("a next run"),
    );
    assert!(resumed_next_run > before_restore);
    assert_eq!(restored_detail["archived"], json!(null));
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_card_id).await)[0],
        json!({ "from": null, "to": "To Do", "reason": "restored", "taskTitle": null })
    );
}

#[sqlx::test]
async fn lists_archived_cards_newest_first_and_leaves_them_off_the_board(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["Older"]), ("Doing", &["Newer"])],
    )
    .await;
    let doing_list_id = list_id(&board, "Doing").to_owned();
    for extra_number in 0..50 {
        add_card(
            &db_pool,
            &signed_in_person,
            &doing_list_id,
            &format!("Extra {extra_number}"),
        )
        .await;
    }
    let full_board = read_board(&db_pool, &signed_in_person, &board).await;
    archive_card(&db_pool, &signed_in_person, card_id(&board, "Older")).await;
    archive_card(&db_pool, &signed_in_person, card_id(&board, "Newer")).await;

    let two_archived = list_archived(&db_pool, &signed_in_person, board_id(&board), "").await;
    let board_after = read_board(&db_pool, &signed_in_person, &board).await;
    for extra_number in 0..50 {
        archive_card(
            &db_pool,
            &signed_in_person,
            card_id(&full_board, &format!("Extra {extra_number}")),
        )
        .await;
    }
    let first_page = list_archived(&db_pool, &signed_in_person, board_id(&board), "").await;
    let oldest_shown_id = first_page["cards"][49]["id"].as_str().expect("an id");
    let second_page = list_archived(
        &db_pool,
        &signed_in_person,
        board_id(&board),
        &format!("before={oldest_shown_id}"),
    )
    .await;

    let newer_card = &two_archived["cards"][0];
    assert_eq!(newer_card["title"], "Newer");
    assert_eq!(newer_card["fromListName"], "Doing");
    assert_eq!(newer_card["how"], "byHand");
    assert!(newer_card["archivedAt"].is_string());
    assert_eq!(two_archived["cards"][1]["title"], "Older");
    assert_eq!(two_archived["hasMore"], false);
    assert!(card_titles_by_list(&board_after, "To Do").is_empty());
    assert!(!card_titles_by_list(&board_after, "Doing").contains(&String::from("Newer")));
    assert_eq!(first_page["cards"].as_array().map(Vec::len), Some(50));
    assert_eq!(first_page["hasMore"], true);
    assert_eq!(
        second_page["cards"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|archived_card| archived_card["title"].clone())
            .collect::<Vec<_>>(),
        vec![json!("Newer"), json!("Older")]
    );
    assert_eq!(second_page["hasMore"], false);
}

#[sqlx::test]
async fn restores_to_the_first_column_after_its_column_was_deleted(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["Plan"]), ("Doing", &["Release"])],
    )
    .await;
    let release_card_id = card_id(&board, "Release").to_owned();
    archive_card(&db_pool, &signed_in_person, &release_card_id).await;
    delete_list(&db_pool, &signed_in_person, list_id(&board, "Doing")).await;

    let archived_after_delete =
        list_archived(&db_pool, &signed_in_person, board_id(&board), "").await;
    let restored_board = restore_card(&db_pool, &signed_in_person, &release_card_id).await;

    assert_eq!(archived_after_delete["cards"][0]["fromListName"], "Doing");
    assert_eq!(
        card_titles_by_list(&restored_board, "To Do"),
        vec!["Release", "Plan"]
    );
}

#[sqlx::test]
async fn refuses_to_restore_onto_a_board_without_columns(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("To Do", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();
    archive_card(&db_pool, &signed_in_person, &release_card_id).await;
    delete_list(&db_pool, &signed_in_person, list_id(&board, "To Do")).await;

    let response = post_card_action(&db_pool, &signed_in_person, &release_card_id, "restore").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "Add a column first." })
    );
}

#[sqlx::test]
async fn triggers_and_schedule_runs_leave_an_archived_card_alone_and_it_can_be_deleted(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To Do", &["Release"]), ("Doing", &[])],
    )
    .await;
    let release_card_id = card_id(&board, "Release").to_owned();
    let doing_list_id = list_id(&board, "Doing").to_owned();
    save_triggers(
        &db_pool,
        &signed_in_person,
        &release_card_id,
        json!({ "startedListId": doing_list_id }),
    )
    .await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &release_card_id,
        weekly_review(&doing_list_id),
    )
    .await;
    let group_tasks =
        import_linked_tasks(&db_pool, &signed_in_person, &release_card_id, &["Ship"]).await;
    archive_card(&db_pool, &signed_in_person, &release_card_id).await;

    let task_run = start_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Ship"),
    )
    .await;
    let run_now_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{}/run-now", schedule_id(&weekly_schedule)),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;

    assert!(!event_id(&task_run).is_empty());
    assert_eq!(run_now_response.status(), StatusCode::OK);
    assert!(read_card(&db_pool, &signed_in_person, &release_card_id).await["archived"].is_object());
    assert_eq!(
        move_shapes(&read_history(&db_pool, &signed_in_person, &release_card_id).await),
        vec![json!({ "from": "To Do", "to": null, "reason": "archivedByHand", "taskTitle": null })]
    );

    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{release_card_id}"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(delete_response.status(), StatusCode::OK);
    assert!(
        list_archived(&db_pool, &signed_in_person, board_id(&board), "").await["cards"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_card_or_board(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("To Do", &["Release"])]).await;
    let release_card_id = card_id(&board, "Release").to_owned();

    let archive_response =
        post_card_action(&db_pool, &other_person, &release_card_id, "archive").await;
    archive_card(&db_pool, &owner, &release_card_id).await;
    let restore_response =
        post_card_action(&db_pool, &other_person, &release_card_id, "restore").await;
    let list_response = get_archived_cards(&db_pool, &other_person, board_id(&board), "").await;

    assert_eq!(archive_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(restore_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(list_response.status(), StatusCode::NOT_FOUND);
}
