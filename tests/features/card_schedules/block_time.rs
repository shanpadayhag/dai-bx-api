use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::card_schedules::create::{
    board_with_review_card, create_schedule, merged, once_at, post_schedule, schedule_id,
    weekly_review,
};
use crate::card_schedules::reads::calendar_week;
use crate::card_schedules::update::patch_schedule;
use crate::support::{SignedInPerson, response_json, sign_up_with_authenticator};

async fn block_times(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    monday: &str,
) -> Vec<(String, String)> {
    calendar_week(db_pool, signed_in_person, monday).await["events"]
        .as_array()
        .expect("a list of events")
        .iter()
        .map(|event_json| {
            (
                event_json["startsAt"].as_str().expect("a start").to_owned(),
                event_json["endsAt"].as_str().expect("an end").to_owned(),
            )
        })
        .collect()
}

fn block_times_of(start: &str, end: &str) -> Vec<(String, String)> {
    vec![(start.to_owned(), end.to_owned())]
}

#[sqlx::test]
async fn a_repeating_block_starts_at_its_own_time_and_moves_when_it_changes(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            weekly_review(&scheduled_card.to_do_list_id),
            json!({ "block": { "minutes": 60, "time": "20:00" } }),
        ),
    )
    .await;
    let first_blocks = block_times(&db_pool, &signed_in_person, "2030-01-07").await;

    let changed_response = patch_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        merged(
            weekly_review(&scheduled_card.to_do_list_id),
            json!({ "block": { "minutes": 60, "time": "21:00" } }),
        ),
    )
    .await;
    assert_eq!(changed_response.status(), StatusCode::OK);
    let changed_schedules = response_json(changed_response).await;

    assert_eq!(
        weekly_schedule["block"],
        json!({ "minutes": 60, "categoryId": null, "time": "20:00" })
    );
    assert_eq!(
        first_blocks,
        block_times_of("2030-01-07T12:00:00.000Z", "2030-01-07T13:00:00.000Z")
    );
    assert_eq!(changed_schedules["schedules"][0]["block"]["time"], "21:00");
    assert_eq!(
        block_times(&db_pool, &signed_in_person, "2030-01-14").await,
        block_times_of("2030-01-14T13:00:00.000Z", "2030-01-14T14:00:00.000Z")
    );
}

#[sqlx::test]
async fn a_once_block_sits_at_its_date_and_own_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;

    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            once_at(&scheduled_card.doing_list_id, "2030-01-09T12:00"),
            json!({ "block": { "minutes": 30, "time": "15:00" } }),
        ),
    )
    .await;

    assert_eq!(once_schedule["block"]["time"], "15:00");
    assert_eq!(
        block_times(&db_pool, &signed_in_person, "2030-01-07").await,
        block_times_of("2030-01-09T15:00:00.000Z", "2030-01-09T15:30:00.000Z")
    );
}

#[sqlx::test]
async fn a_block_without_a_time_starts_at_the_run(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;

    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        merged(
            once_at(&scheduled_card.doing_list_id, "2030-01-09T12:00"),
            json!({ "block": { "minutes": 30, "time": null } }),
        ),
    )
    .await;

    assert_eq!(once_schedule["block"]["time"], serde_json::Value::Null);
    assert_eq!(
        block_times(&db_pool, &signed_in_person, "2030-01-07").await,
        block_times_of("2030-01-09T12:00:00.000Z", "2030-01-09T12:30:00.000Z")
    );
}

#[sqlx::test]
async fn refuses_a_bad_block_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;

    for refused_block in [
        json!({ "minutes": 30, "time": "9am" }),
        json!({ "minutes": 30, "time": "25:00" }),
        json!({ "time": "10:00" }),
    ] {
        let response = post_schedule(
            &db_pool,
            &signed_in_person,
            &scheduled_card.card_id,
            merged(
                once_at(&scheduled_card.doing_list_id, "2030-01-09T12:00"),
                json!({ "block": refused_block }),
            ),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_block}"
        );
    }
}
