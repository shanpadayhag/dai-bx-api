use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id};
use crate::cards::create::{board_with_cards, card_id, card_titles_by_list};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn move_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    list_id: &str,
    position: i64,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/move"),
        json!({ "listId": list_id, "position": position }),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn moved_board(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    list_id: &str,
    position: i64,
) -> serde_json::Value {
    let response = move_card(db_pool, signed_in_person, card_id, list_id, position).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn reorders_a_card_within_its_list(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Backlog", &["A", "B", "C", "D"])],
    )
    .await;
    let backlog_list_id = list_id(&board, "Backlog");

    let moved_down = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "A"),
        backlog_list_id,
        2,
    )
    .await;
    let moved_up = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "D"),
        backlog_list_id,
        0,
    )
    .await;
    let clamped = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "B"),
        backlog_list_id,
        99,
    )
    .await;

    assert_eq!(
        card_titles_by_list(&moved_down, "Backlog"),
        vec!["B", "C", "A", "D"]
    );
    assert_eq!(
        card_titles_by_list(&moved_up, "Backlog"),
        vec!["D", "B", "C", "A"]
    );
    assert_eq!(
        card_titles_by_list(&clamped, "Backlog"),
        vec!["D", "C", "A", "B"]
    );
}

#[sqlx::test]
async fn moves_a_card_to_another_list_and_renumbers_both(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[
            ("Backlog", &["A", "B", "C"]),
            ("Done", &["X", "Y"]),
            ("Empty", &[]),
        ],
    )
    .await;

    let into_the_middle = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "A"),
        list_id(&board, "Done"),
        1,
    )
    .await;
    let past_the_end = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "C"),
        list_id(&board, "Done"),
        99,
    )
    .await;
    let into_an_empty_list = moved_board(
        &db_pool,
        &signed_in_person,
        card_id(&board, "X"),
        list_id(&board, "Empty"),
        3,
    )
    .await;

    assert_eq!(
        card_titles_by_list(&into_the_middle, "Backlog"),
        vec!["B", "C"]
    );
    assert_eq!(
        card_titles_by_list(&into_the_middle, "Done"),
        vec!["X", "A", "Y"]
    );
    assert_eq!(card_titles_by_list(&past_the_end, "Backlog"), vec!["B"]);
    assert_eq!(
        card_titles_by_list(&past_the_end, "Done"),
        vec!["X", "A", "Y", "C"]
    );
    assert_eq!(
        card_titles_by_list(&into_an_empty_list, "Done"),
        vec!["A", "Y", "C"]
    );
    assert_eq!(card_titles_by_list(&into_an_empty_list, "Empty"), vec!["X"]);
}

#[sqlx::test]
async fn refuses_a_move_to_another_board_s_list_or_a_negative_position(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("Backlog", &["A"])]).await;
    let owner_other_board = board_with_lists(&db_pool, &owner, &["Elsewhere"]).await;
    let foreign_board = board_with_lists(&db_pool, &other_person, &["Theirs"]).await;
    let card_to_move = card_id(&board, "A");

    for (target_list_id, position, expected_error) in [
        (
            list_id(&owner_other_board, "Elsewhere"),
            0,
            "a card moves only within its board",
        ),
        (
            list_id(&foreign_board, "Theirs"),
            0,
            "a card moves only within its board",
        ),
        (list_id(&board, "Backlog"), -1, "a position is 0 or more"),
    ] {
        let response = move_card(&db_pool, &owner, card_to_move, target_list_id, position).await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": expected_error })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("Backlog", &["A"])]).await;

    let response = move_card(
        &db_pool,
        &other_person,
        card_id(&board, "A"),
        list_id(&board, "Backlog"),
        0,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
