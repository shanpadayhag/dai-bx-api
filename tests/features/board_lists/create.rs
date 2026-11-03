use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn add_list(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_id: &str,
    list_name: &str,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/boards/{board_id}/lists"),
        json!({ "name": list_name }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn board_with_lists(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    list_names: &[&str],
) -> serde_json::Value {
    let created_board = create_board(db_pool, signed_in_person, "Personal").await;
    let mut board = created_board.clone();
    for list_name in list_names {
        board = add_list(
            db_pool,
            signed_in_person,
            board_id(&created_board),
            list_name,
        )
        .await;
    }
    board
}

pub fn list_id<'a>(board: &'a serde_json::Value, list_name: &str) -> &'a str {
    board["lists"]
        .as_array()
        .expect("a list array")
        .iter()
        .find(|board_list| board_list["name"] == list_name)
        .and_then(|board_list| board_list["id"].as_str())
        .expect("the list should be on the board")
}

pub fn list_places(board: &serde_json::Value) -> Vec<(String, i64)> {
    board["lists"]
        .as_array()
        .expect("a list array")
        .iter()
        .map(|board_list| {
            (
                board_list["name"].as_str().expect("a name").to_owned(),
                board_list["position"].as_i64().expect("a position"),
            )
        })
        .collect()
}

pub fn places(expected_names: &[&str]) -> Vec<(String, i64)> {
    expected_names
        .iter()
        .zip(0..)
        .map(|(list_name, position)| ((*list_name).to_owned(), position))
        .collect()
}

#[sqlx::test]
async fn adds_each_new_list_last_with_a_trimmed_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let board = board_with_lists(
        &db_pool,
        &signed_in_person,
        &["  Backlog ", "To do", "Done"],
    )
    .await;

    assert_eq!(list_places(&board), places(&["Backlog", "To do", "Done"]));
    assert_eq!(board["board"]["name"], "Personal");
    assert_eq!(board["cards"], json!([]));
}

#[sqlx::test]
async fn refuses_a_list_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    for rejected_name in ["  ", &"a".repeat(101)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/boards/{}/lists", board_id(&created_board)),
            json!({ "name": rejected_name }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a list name is 1 to 100 characters" })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_board_or_a_malformed_id(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_board = create_board(&db_pool, &owner, "Mine").await;

    for (lists_path, cookie_header) in [
        (
            format!("/boards/{}/lists", board_id(&owner_board)),
            &other_person.session_cookies,
        ),
        (
            String::from("/boards/not-a-uuid/lists"),
            &owner.session_cookies,
        ),
    ] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &lists_path,
            json!({ "name": "Backlog" }),
            cookie_header,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn adding_a_list_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{}/lists", board_id(&created_board)),
        json!({ "name": "Backlog" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
