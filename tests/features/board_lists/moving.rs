use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id, list_places, places};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn move_list(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    list_id: &str,
    position: i64,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{list_id}/move"),
        json!({ "position": position }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn moves_a_list_and_keeps_every_position_contiguous(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["A", "B", "C", "D"]).await;

    let moved_right = move_list(&db_pool, &signed_in_person, list_id(&board, "A"), 2).await;
    let moved_left = move_list(&db_pool, &signed_in_person, list_id(&board, "D"), 0).await;
    let moved_in_place = move_list(&db_pool, &signed_in_person, list_id(&board, "C"), 2).await;

    assert_eq!(list_places(&moved_right), places(&["B", "C", "A", "D"]));
    assert_eq!(list_places(&moved_left), places(&["D", "B", "C", "A"]));
    assert_eq!(list_places(&moved_in_place), places(&["D", "B", "C", "A"]));
}

#[sqlx::test]
async fn clamps_a_position_past_the_end_to_last(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["A", "B", "C"]).await;

    let moved_board = move_list(&db_pool, &signed_in_person, list_id(&board, "A"), 99).await;

    assert_eq!(list_places(&moved_board), places(&["B", "C", "A"]));
}

#[sqlx::test]
async fn refuses_a_negative_position(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["A", "B"]).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}/move", list_id(&board, "B")),
        json!({ "position": -1 }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "a position is 0 or more" })
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_list(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &owner, &["A", "B"]).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}/move", list_id(&board, "A")),
        json!({ "position": 1 }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
