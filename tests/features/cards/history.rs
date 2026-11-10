use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::{
    board_with_review_card, create_schedule, once_at, schedule_id,
};
use crate::cards::create::add_card;
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn get_history(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    query: &str,
) -> axum::response::Response {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/history?{query}"),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn read_history(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
) -> serde_json::Value {
    let response = get_history(db_pool, signed_in_person, card_id, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub fn move_shapes(history: &serde_json::Value) -> Vec<serde_json::Value> {
    history["moves"]
        .as_array()
        .expect("a list of moves")
        .iter()
        .map(|card_move| {
            json!({
                "from": card_move["fromListName"],
                "to": card_move["toListName"],
                "reason": card_move["reason"],
                "taskTitle": card_move["taskTitle"],
            })
        })
        .collect()
}

pub fn shape(from: &str, to: &str, reason: &str) -> serde_json::Value {
    json!({ "from": from, "to": to, "reason": reason, "taskTitle": null })
}

pub async fn drag_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    list_id: &str,
    position: i64,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/move"),
        json!({ "listId": list_id, "position": position }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn run_now(db_pool: &PgPool, signed_in_person: &SignedInPerson, schedule_id: &str) {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{schedule_id}/run-now"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn records_a_drag_a_reorder_and_a_schedule_run_that_moves(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    add_card(
        &db_pool,
        &signed_in_person,
        &scheduled_card.doing_list_id,
        "Neighbour",
    )
    .await;
    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.to_do_list_id, "2030-01-07T10:00"),
    )
    .await;
    assert_eq!(
        read_history(&db_pool, &signed_in_person, &scheduled_card.card_id).await,
        json!({ "moves": [], "hasMore": false })
    );

    for (target_list_id, position) in [
        (&scheduled_card.doing_list_id, 1),
        (&scheduled_card.to_do_list_id, 0),
        (&scheduled_card.doing_list_id, 0),
    ] {
        drag_card(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            target_list_id,
            position,
        )
        .await;
    }
    run_now(&db_pool, &signed_in_person, schedule_id(&once_schedule)).await;
    let history = read_history(&db_pool, &signed_in_person, &scheduled_card.card_id).await;

    assert_eq!(
        move_shapes(&history),
        vec![
            shape("Doing", "To Do", "schedule"),
            shape("To Do", "Doing", "byHand"),
            shape("Doing", "To Do", "byHand"),
            shape("Doing", "Doing", "byHand"),
        ]
    );
    assert_eq!(history["hasMore"], false);
}

#[sqlx::test]
async fn a_drop_back_in_the_same_place_or_a_run_that_finds_the_card_there_adds_nothing(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.doing_list_id, "2030-01-07T10:00"),
    )
    .await;

    drag_card(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        &scheduled_card.doing_list_id,
        0,
    )
    .await;
    run_now(&db_pool, &signed_in_person, schedule_id(&once_schedule)).await;

    assert_eq!(
        read_history(&db_pool, &signed_in_person, &scheduled_card.card_id).await["moves"],
        json!([])
    );
}

#[sqlx::test]
async fn pages_history_newest_first_fifty_at_a_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let bounce_list_ids = [&scheduled_card.to_do_list_id, &scheduled_card.doing_list_id];
    for move_number in 0..52 {
        drag_card(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            bounce_list_ids[move_number % 2],
            0,
        )
        .await;
    }

    let first_page = read_history(&db_pool, &signed_in_person, &scheduled_card.card_id).await;
    let first_moves = first_page["moves"].as_array().expect("a list");
    let oldest_shown_id = first_moves.last().expect("a move")["id"]
        .as_str()
        .expect("an id");
    let second_page = response_json(
        get_history(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            &format!("before={oldest_shown_id}"),
        )
        .await,
    )
    .await;
    let small_page = response_json(
        get_history(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            "limit=1",
        )
        .await,
    )
    .await;

    assert_eq!(first_moves.len(), 50);
    assert_eq!(first_page["hasMore"], true);
    assert_eq!(first_moves[0]["toListName"], "Doing");
    assert_eq!(
        move_shapes(&second_page),
        vec![
            shape("To Do", "Doing", "byHand"),
            shape("Doing", "To Do", "byHand"),
        ]
    );
    assert_eq!(second_page["hasMore"], false);
    assert_eq!(small_page["moves"].as_array().map(Vec::len), Some(1));
    assert_eq!(small_page["hasMore"], true);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_history_and_refuses_bad_paging(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &owner).await;

    let foreign_response = get_history(&db_pool, &other_person, &scheduled_card.card_id, "").await;
    let bad_limit_response =
        get_history(&db_pool, &owner, &scheduled_card.card_id, "limit=51").await;
    let bad_before_response = get_history(
        &db_pool,
        &owner,
        &scheduled_card.card_id,
        "before=yesterday",
    )
    .await;

    assert_eq!(foreign_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        bad_limit_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        bad_before_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
