use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::{
    board_with_review_card, create_schedule, iso_millis, merged, now_millis, schedule_id,
    weekly_review,
};
use crate::card_schedules::reads::{calendar_week, read_board};
use crate::cards::create::card;
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn post_pause_or_resume(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
    pause_or_resume: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{schedule_id}/{pause_or_resume}"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn pause_schedule(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
    pause_or_resume: &str,
) -> serde_json::Value {
    let response =
        post_pause_or_resume(db_pool, signed_in_person, schedule_id, pause_or_resume).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn hides_a_paused_schedule_s_blocks_and_moves_until_it_resumes(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            weekly_review(&scheduled_card.to_do_list_id),
            json!({ "block": { "minutes": 60 } }),
        ),
    )
    .await;

    let paused_schedule = pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        "pause",
    )
    .await;
    let paused_week = calendar_week(&db_pool, &signed_in_person, "2030-01-14").await;
    let paused_board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    let resumed_schedule = pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        "resume",
    )
    .await;
    let resumed_week = calendar_week(&db_pool, &signed_in_person, "2030-01-14").await;

    assert_eq!(paused_schedule["paused"], true);
    assert_eq!(paused_week["events"], json!([]));
    assert_eq!(paused_week["moves"], json!([]));
    assert_eq!(
        card(&paused_board, "Weekly review")["nextMove"],
        serde_json::Value::Null
    );
    assert_eq!(resumed_schedule, weekly_schedule);
    assert_eq!(resumed_week["events"].as_array().map(Vec::len), Some(1));
    assert_eq!(resumed_week["moves"].as_array().map(Vec::len), Some(1));
}

#[sqlx::test]
async fn resumes_from_the_next_future_run_without_catching_up(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let daily_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        json!({
            "kind": "repeating",
            "targetListId": scheduled_card.to_do_list_id,
            "start": "2026-01-05T09:00",
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
        }),
    )
    .await;
    pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&daily_schedule),
        "pause",
    )
    .await;
    let before_resume = now_millis();

    let resumed_schedule = pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&daily_schedule),
        "resume",
    )
    .await;

    let next_run_millis = iso_millis(resumed_schedule["nextRunAt"].as_str().expect("a next run"));
    assert_eq!(resumed_schedule["paused"], false);
    assert!(next_run_millis > before_resume);
    assert!(next_run_millis <= before_resume + 86_400_000);
}

#[sqlx::test]
async fn answers_not_found_when_pausing_someone_else_s_schedule(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    for pause_or_resume in ["pause", "resume"] {
        for (wanted_schedule_id, signed_in) in [
            (schedule_id(&weekly_schedule).to_owned(), &other_person),
            (uuid::Uuid::now_v7().to_string(), &signed_in_person),
            (String::from("not-a-uuid"), &signed_in_person),
        ] {
            let response =
                post_pause_or_resume(&db_pool, signed_in, &wanted_schedule_id, pause_or_resume)
                    .await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{pause_or_resume} {wanted_schedule_id}"
            );
        }
    }
    let unchanged_schedule = pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        "resume",
    )
    .await;
    assert_eq!(unchanged_schedule, weekly_schedule);
}
