use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::board_id;
use crate::card_schedules::create::{
    FIRST_MONDAY, board_with_review_card, create_schedule, merged, once_at, schedule_id,
    weekly_review,
};
use crate::card_schedules::pause::pause_schedule;
use crate::cards::create::{add_card, card};
use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, response_json, sign_up_with_authenticator,
};

pub async fn calendar_week(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    monday: &str,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/events?start={monday}&days=7&timeZone=UTC"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn read_board(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_json: &serde_json::Value,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/boards/{}", board_id(board_json)),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn shows_the_soonest_active_schedule_on_the_board_tile(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    add_card(
        &db_pool,
        &signed_in_person,
        &scheduled_card.doing_list_id,
        "Plan",
    )
    .await;
    create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.doing_list_id, "2030-03-01T09:00"),
    )
    .await;
    create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let sooner_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.doing_list_id, "2030-01-03T09:00"),
    )
    .await;
    pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&sooner_schedule),
        "pause",
    )
    .await;

    let board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;

    assert_eq!(
        card(&board, "Weekly review")["nextMove"],
        json!({ "at": FIRST_MONDAY, "listName": "To Do" })
    );
    assert_eq!(card(&board, "Plan")["nextMove"], serde_json::Value::Null);
}

#[sqlx::test]
async fn shows_moves_and_blocks_ahead_on_the_calendar(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            weekly_review(&scheduled_card.to_do_list_id),
            json!({ "block": { "minutes": 60, "categoryId": studies_category_id } }),
        ),
    )
    .await;
    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            once_at(&scheduled_card.doing_list_id, "2030-01-09T12:00"),
            json!({ "block": { "minutes": 30 } }),
        ),
    )
    .await;

    let first_week = calendar_week(&db_pool, &signed_in_person, "2030-01-07").await;
    let second_week = calendar_week(&db_pool, &signed_in_person, "2030-01-14").await;

    let shown_blocks: Vec<(String, String, String, serde_json::Value)> = first_week["events"]
        .as_array()
        .expect("a list of events")
        .iter()
        .map(|event_json| {
            (
                event_json["title"].as_str().expect("a title").to_owned(),
                event_json["startsAt"].as_str().expect("a start").to_owned(),
                event_json["endsAt"].as_str().expect("an end").to_owned(),
                event_json["categoryId"].clone(),
            )
        })
        .collect();
    assert_eq!(
        shown_blocks,
        vec![
            (
                String::from("Weekly review"),
                String::from(FIRST_MONDAY),
                String::from("2030-01-07T11:00:00.000Z"),
                json!(studies_category_id),
            ),
            (
                String::from("Weekly review"),
                String::from("2030-01-09T12:00:00.000Z"),
                String::from("2030-01-09T12:30:00.000Z"),
                serde_json::Value::Null,
            ),
        ]
    );
    assert!(first_week["events"][0]["series"].is_object());
    let board_id_text = board_id(&scheduled_card.board);
    assert_eq!(
        first_week["moves"],
        json!([
            {
                "scheduleId": weekly_schedule["id"],
                "cardId": scheduled_card.card_id,
                "cardTitle": "Weekly review",
                "boardId": board_id_text,
                "listName": "To Do",
                "at": FIRST_MONDAY,
            },
            {
                "scheduleId": once_schedule["id"],
                "cardId": scheduled_card.card_id,
                "cardTitle": "Weekly review",
                "boardId": board_id_text,
                "listName": "Doing",
                "at": "2030-01-09T12:00:00.000Z",
            },
        ])
    );
    assert_eq!(second_week["events"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_week["moves"][0]["at"], "2030-01-14T10:00:00.000Z");
}
