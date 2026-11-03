use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id};
use crate::boards::create::board_id;
use crate::cards::create::{add_card, card, card_id};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{find_task_named, task_id_named};
use crate::tasks::import::{import_group, list_group_names};

async fn board_with_two_cards(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let board = board_with_lists(db_pool, signed_in_person, &["Backlog"]).await;
    let backlog_list_id = list_id(&board, "Backlog").to_owned();
    add_card(
        db_pool,
        signed_in_person,
        &backlog_list_id,
        "Write the spec",
    )
    .await;
    add_card(db_pool, signed_in_person, &backlog_list_id, "Plan the week").await
}

async fn import_linked_group(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board: &serde_json::Value,
) -> serde_json::Value {
    let response = import_group(
        db_pool,
        signed_in_person,
        json!({
            "name": "Today",
            "tasks": [
                {
                    "name": "Spec – draft",
                    "card": card_id(board, "Write the spec"),
                    "tasks": [
                        { "name": "Outline", "card": card_id(board, "Plan the week"), "tasks": [] },
                    ],
                },
                { "name": "Spec – review", "card": card_id(board, "Write the spec"), "tasks": [] },
                { "name": "Inbox zero", "tasks": [] },
            ],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn read_board(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board: &serde_json::Value,
) -> serde_json::Value {
    response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/boards/{}", board_id(board)),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await
}

fn linked_counts(board: &serde_json::Value, card_title: &str) -> (i64, i64) {
    let board_card = card(board, card_title);
    (
        board_card["linkedTaskCount"].as_i64().expect("a count"),
        board_card["doneLinkedTaskCount"].as_i64().expect("a count"),
    )
}

#[sqlx::test]
async fn links_each_top_level_task_and_ignores_a_subtask_s_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_two_cards(&db_pool, &signed_in_person).await;

    let group_tasks = import_linked_group(&db_pool, &signed_in_person, &board).await;

    let spec_link = json!({
        "id": card_id(&board, "Write the spec"),
        "title": "Write the spec",
        "boardId": board_id(&board),
    });
    assert_eq!(
        find_task_named(&group_tasks, "Spec – draft")["card"],
        spec_link
    );
    assert_eq!(
        find_task_named(&group_tasks, "Spec – review")["card"],
        spec_link
    );
    assert_eq!(
        find_task_named(&group_tasks, "Outline")["card"],
        json!(null)
    );
    assert_eq!(
        find_task_named(&group_tasks, "Inbox zero")["card"],
        json!(null)
    );

    let card_detail = response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/cards/{}", card_id(&board, "Write the spec")),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    let linked_task_names: Vec<&str> = card_detail["linkedTasks"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|linked_task| linked_task["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(linked_task_names, vec!["Spec – draft", "Spec – review"]);
    assert_eq!(card_detail["linkedTasks"][0]["groupName"], "Today");
}

#[sqlx::test]
async fn writes_nothing_when_a_card_reference_is_bad_and_names_where(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_board = board_with_two_cards(&db_pool, &owner).await;
    let other_board = board_with_two_cards(&db_pool, &other_person).await;

    for (bad_card, expected_path) in [
        (
            json!(card_id(&other_board, "Write the spec")),
            "tasks[3].card",
        ),
        (json!("not-a-uuid"), "tasks[3].card"),
        (json!(uuid_like_but_missing()), "tasks[3].card"),
    ] {
        let response = import_group(
            &db_pool,
            &owner,
            json!({
                "name": "Today",
                "tasks": [
                    { "name": "A", "card": card_id(&owner_board, "Write the spec"), "tasks": [] },
                    { "name": "B", "tasks": [] },
                    { "name": "C", "card": card_id(&owner_board, "Plan the week"), "tasks": [] },
                    { "name": "D", "card": bad_card, "tasks": [] },
                ],
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "no card of yours has that id", "path": expected_path })
        );
    }
    assert_eq!(
        list_group_names(&db_pool, &owner).await,
        Vec::<String>::new()
    );
}

fn uuid_like_but_missing() -> String {
    String::from("01900000-0000-7000-8000-000000000000")
}

#[sqlx::test]
async fn counts_follow_completing_and_reopening_linked_tasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_two_cards(&db_pool, &signed_in_person).await;
    let group_tasks = import_linked_group(&db_pool, &signed_in_person, &board).await;
    let draft_task_id = task_id_named(&group_tasks, "Spec – draft");

    let imported_counts = linked_counts(
        &read_board(&db_pool, &signed_in_person, &board).await,
        "Write the spec",
    );
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, true).await;
    let completed_counts = linked_counts(
        &read_board(&db_pool, &signed_in_person, &board).await,
        "Write the spec",
    );
    set_task_done(&db_pool, &signed_in_person, &draft_task_id, false).await;
    let reopened_counts = linked_counts(
        &read_board(&db_pool, &signed_in_person, &board).await,
        "Write the spec",
    );

    assert_eq!(imported_counts, (2, 0));
    assert_eq!(completed_counts, (2, 1));
    assert_eq!(reopened_counts, (2, 0));
    assert_eq!(
        linked_counts(
            &read_board(&db_pool, &signed_in_person, &board).await,
            "Plan the week"
        ),
        (0, 0)
    );
}

#[sqlx::test]
async fn deleting_the_card_keeps_its_tasks_without_a_link(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_two_cards(&db_pool, &signed_in_person).await;
    let group_tasks = import_linked_group(&db_pool, &signed_in_person, &board).await;
    let group_id = group_tasks["group"]["id"].as_str().expect("a group id");

    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{}", card_id(&board, "Write the spec")),
        &signed_in_person.session_cookies,
    )
    .await;
    let group_after_delete = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            &format!("/task-groups/{group_id}/tasks"),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(delete_response.status(), StatusCode::OK);
    assert_eq!(
        group_after_delete["tasks"]
            .as_array()
            .expect("a list")
            .len(),
        4
    );
    assert_eq!(
        find_task_named(&group_after_delete, "Spec – draft")["card"],
        json!(null)
    );
    assert_eq!(
        find_task_named(&group_after_delete, "Spec – review")["card"],
        json!(null)
    );
}
