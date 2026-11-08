use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub const NEW_CARD_NOTES: &str = "## Outcome\n\n## Context\n\n## Open questions\n\n## Done when\n";

pub async fn add_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    list_id: &str,
    card_title: &str,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{list_id}/cards"),
        json!({ "title": card_title }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn board_with_cards(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    cards_by_list: &[(&str, &[&str])],
) -> serde_json::Value {
    let list_names: Vec<&str> = cards_by_list
        .iter()
        .map(|(list_name, _)| *list_name)
        .collect();
    let mut board = board_with_lists(db_pool, signed_in_person, &list_names).await;
    for (list_name, card_titles) in cards_by_list {
        let target_list_id = list_id(&board, list_name).to_owned();
        for card_title in *card_titles {
            board = add_card(db_pool, signed_in_person, &target_list_id, card_title).await;
        }
    }
    board
}

pub fn card<'a>(board: &'a serde_json::Value, card_title: &str) -> &'a serde_json::Value {
    board["cards"]
        .as_array()
        .expect("a card array")
        .iter()
        .find(|board_card| board_card["title"] == card_title)
        .expect("the card should be on the board")
}

pub fn card_id<'a>(board: &'a serde_json::Value, card_title: &str) -> &'a str {
    card(board, card_title)["id"].as_str().expect("a card id")
}

pub fn card_titles_by_list(board: &serde_json::Value, list_name: &str) -> Vec<String> {
    let wanted_list_id = list_id(board, list_name);
    let mut list_cards: Vec<(i64, String)> = board["cards"]
        .as_array()
        .expect("a card array")
        .iter()
        .filter(|board_card| board_card["listId"] == wanted_list_id)
        .map(|board_card| {
            (
                board_card["position"].as_i64().expect("a position"),
                board_card["title"].as_str().expect("a title").to_owned(),
            )
        })
        .collect();
    list_cards.sort();
    let positions: Vec<i64> = list_cards.iter().map(|(position, _)| *position).collect();
    assert_eq!(
        positions,
        (0..list_cards.len() as i64).collect::<Vec<_>>(),
        "positions in {list_name} should run 0, 1, 2… with no gaps"
    );
    list_cards
        .into_iter()
        .map(|(_, card_title)| card_title)
        .collect()
}

#[sqlx::test]
async fn adds_each_card_last_in_its_list_with_a_trimmed_title(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[
            ("Backlog", &["  Write the spec ", "Plan the week"]),
            ("Done", &["Ship it"]),
        ],
    )
    .await;

    assert_eq!(
        card_titles_by_list(&board, "Backlog"),
        vec!["Write the spec", "Plan the week"]
    );
    assert_eq!(card_titles_by_list(&board, "Done"), vec!["Ship it"]);
    assert_eq!(
        card(&board, "Ship it"),
        &json!({
            "id": card_id(&board, "Ship it"),
            "listId": list_id(&board, "Done"),
            "position": 0,
            "title": "Ship it",
            "dueDate": null,
            "dueTime": null,
            "values": [],
            "linkedTaskCount": 0,
            "doneLinkedTaskCount": 0,
            "nextMove": null,
        })
    );
}

#[sqlx::test]
async fn starts_a_new_card_s_notes_from_the_outline(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;

    let card_detail = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            &format!("/cards/{}", card_id(&board, "Plan")),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(card_detail["notes"], NEW_CARD_NOTES);
}

#[sqlx::test]
async fn refuses_a_title_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Backlog"]).await;

    for rejected_title in ["   ", &"a".repeat(501)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/lists/{}/cards", list_id(&board, "Backlog")),
            json!({ "title": rejected_title }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a card title is 1 to 500 characters" })
        );
    }
    add_card(
        &db_pool,
        &signed_in_person,
        list_id(&board, "Backlog"),
        &"a".repeat(500),
    )
    .await;
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_list(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &owner, &["Backlog"]).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}/cards", list_id(&board, "Backlog")),
        json!({ "title": "Taken" }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn adding_a_card_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Backlog"]).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}/cards", list_id(&board, "Backlog")),
        json!({ "title": "Plan" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
