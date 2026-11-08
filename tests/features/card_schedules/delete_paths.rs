use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::card_schedules::actions::run_due_schedules::RunDueSchedules;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::board_lists::create::{add_list, list_id};
use crate::boards::create::board_id;
use crate::card_schedules::create::{
    ScheduledCard, board_with_review_card, create_schedule, merged, once_at, read_card,
    schedule_id, weekly_review,
};
use crate::card_schedules::pause::pause_schedule;
use crate::card_schedules::reads::calendar_week;
use crate::card_schedules::update::patch_schedule;
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, response_json, sign_up_with_authenticator,
};

fn with_block(schedule_body: serde_json::Value) -> serde_json::Value {
    merged(schedule_body, json!({ "block": { "minutes": 60 } }))
}

async fn delete_path(db_pool: &PgPool, signed_in_person: &SignedInPerson, request_path: &str) {
    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        request_path,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "{request_path}");
}

fn event_titles_and_starts(week: &serde_json::Value) -> Vec<(String, i64)> {
    week["events"]
        .as_array()
        .expect("a list of events")
        .iter()
        .map(|event_json| {
            (
                event_json["title"].as_str().expect("a title").to_owned(),
                crate::card_schedules::create::iso_millis(
                    event_json["startsAt"].as_str().expect("a start"),
                ),
            )
        })
        .collect()
}

#[sqlx::test]
async fn pauses_and_flags_schedules_into_a_deleted_list_until_another_is_picked(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let board_with_someday = add_list(
        &db_pool,
        &signed_in_person,
        board_id(&scheduled_card.board),
        "Someday",
    )
    .await;
    let someday_list_id = list_id(&board_with_someday, "Someday").to_owned();
    let someday_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        with_block(weekly_review(&someday_list_id)),
    )
    .await;
    let someday_schedule_id = schedule_id(&someday_schedule).to_owned();

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/lists/{someday_list_id}"),
    )
    .await;

    let flagged_schedule =
        read_card(&db_pool, &signed_in_person, &scheduled_card.card_id).await["schedules"][0]
            .clone();
    assert_eq!(flagged_schedule["paused"], json!(true));
    assert_eq!(flagged_schedule["targetListId"], json!(null));
    assert_eq!(flagged_schedule["targetListName"], json!(null));
    let flagged_week = calendar_week(&db_pool, &signed_in_person, "2030-01-07").await;
    assert_eq!(flagged_week["events"], json!([]));
    assert_eq!(flagged_week["moves"], json!([]));
    let scheduler_tally = RunDueSchedules::new(&db_pool)
        .execute(UtcInstant::parse("2030-01-07T10:01:00Z").expect("a valid test time"))
        .await
        .expect("the scheduler call should run");
    assert_eq!(scheduler_tally.ran_count, 0);

    let patch_response = patch_schedule(
        &db_pool,
        &signed_in_person,
        &someday_schedule_id,
        with_block(weekly_review(&scheduled_card.to_do_list_id)),
    )
    .await;
    assert_eq!(patch_response.status(), StatusCode::OK);
    let repointed_schedule = response_json(patch_response).await["schedules"][0].clone();
    assert_eq!(repointed_schedule["paused"], json!(true));
    assert_eq!(
        repointed_schedule["targetListId"],
        json!(scheduled_card.to_do_list_id)
    );
    assert_eq!(repointed_schedule["targetListName"], json!("To Do"));
    assert_eq!(
        calendar_week(&db_pool, &signed_in_person, "2030-01-07").await["events"],
        json!([])
    );

    let resumed_schedule =
        pause_schedule(&db_pool, &signed_in_person, &someday_schedule_id, "resume").await;
    assert_eq!(resumed_schedule["paused"], json!(false));
    assert_eq!(
        event_titles_and_starts(&calendar_week(&db_pool, &signed_in_person, "2030-01-07").await),
        vec![(
            String::from("Weekly review"),
            crate::card_schedules::create::iso_millis("2030-01-07T10:00:00Z")
        )]
    );
}

async fn card_with_past_and_future_blocks(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> ScheduledCard {
    let scheduled_card = board_with_review_card(db_pool, signed_in_person).await;
    let weekly_schedule = create_schedule(
        db_pool,
        signed_in_person,
        &scheduled_card.card_id,
        with_block(weekly_review(&scheduled_card.to_do_list_id)),
    )
    .await;
    let earlier_once_schedule = create_schedule(
        db_pool,
        signed_in_person,
        &scheduled_card.card_id,
        with_block(once_at(&scheduled_card.to_do_list_id, "2030-02-01T09:00")),
    )
    .await;
    create_schedule(
        db_pool,
        signed_in_person,
        &scheduled_card.card_id,
        with_block(once_at(&scheduled_card.to_do_list_id, "2030-03-01T09:00")),
    )
    .await;
    let parsed_id = |schedule_json: &serde_json::Value| {
        Uuid::parse_str(schedule_id(schedule_json)).expect("a schedule uuid")
    };
    sqlx::query(
        "UPDATE event_series SET local_start = '2026-10-12T18:00'::timestamp WHERE schedule_id = $1",
    )
    .bind(parsed_id(&weekly_schedule))
    .execute(db_pool)
    .await
    .expect("the series should move into the past");
    sqlx::query(
        "UPDATE calendar_events
         SET starts_at = '2026-10-01T09:00:00Z', ends_at = '2026-10-01T10:00:00Z'
         WHERE schedule_id = $1",
    )
    .bind(parsed_id(&earlier_once_schedule))
    .execute(db_pool)
    .await
    .expect("the once block should move into the past");
    scheduled_card
}

async fn assert_only_past_blocks_remain(db_pool: &PgPool, signed_in_person: &SignedInPerson) {
    let iso_millis = crate::card_schedules::create::iso_millis;
    assert_eq!(
        event_titles_and_starts(&calendar_week(db_pool, signed_in_person, "2026-09-28").await),
        vec![(
            String::from("Weekly review"),
            iso_millis("2026-10-01T09:00:00Z")
        )]
    );
    assert_eq!(
        event_titles_and_starts(&calendar_week(db_pool, signed_in_person, "2026-10-12").await),
        vec![(
            String::from("Weekly review"),
            iso_millis("2026-10-12T10:00:00Z")
        )]
    );
    for future_monday in ["2030-01-07", "2030-02-25"] {
        let future_week = calendar_week(db_pool, signed_in_person, future_monday).await;
        assert_eq!(future_week["events"], json!([]), "{future_monday}");
        assert_eq!(future_week["moves"], json!([]), "{future_monday}");
    }
}

#[sqlx::test]
async fn keeps_past_blocks_of_a_deleted_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = card_with_past_and_future_blocks(&db_pool, &signed_in_person).await;
    assert_eq!(
        event_titles_and_starts(&calendar_week(&db_pool, &signed_in_person, "2030-02-25").await)
            .len(),
        2
    );

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/cards/{}", scheduled_card.card_id),
    )
    .await;

    assert_only_past_blocks_remain(&db_pool, &signed_in_person).await;
}

#[sqlx::test]
async fn keeps_past_blocks_of_the_cards_in_a_deleted_list(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = card_with_past_and_future_blocks(&db_pool, &signed_in_person).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/lists/{}", scheduled_card.doing_list_id),
    )
    .await;

    assert_only_past_blocks_remain(&db_pool, &signed_in_person).await;
}

#[sqlx::test]
async fn keeps_past_blocks_of_the_cards_on_a_deleted_board(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = card_with_past_and_future_blocks(&db_pool, &signed_in_person).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/boards/{}", board_id(&scheduled_card.board)),
    )
    .await;

    assert_only_past_blocks_remain(&db_pool, &signed_in_person).await;
}
