use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::activity::feed::{entry_kinds, list_entries};
use crate::calendar_events::create::event_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::support::{
    build_test_app, post_json_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tracking::finish::{minutes_from_now, post_finish};
use crate::tracking::start::{add_top_level_task, post_start, start_card, start_task};

#[sqlx::test]
async fn starting_a_task_adds_a_started_entry_with_its_names(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;

    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["started"]);
    let started_entry = &activity_entries[0];
    assert_eq!(started_entry["title"], "Write the changelog");
    assert_eq!(started_entry["contextName"], "Today");
    assert_eq!(started_entry["taskId"], task_id.as_str());
    assert_eq!(started_entry["cardId"], serde_json::Value::Null);
    assert_eq!(
        started_entry["details"],
        json!({ "subject": "task", "eventId": running_item["id"] })
    );
}

#[sqlx::test]
async fn starting_a_card_adds_a_started_entry_with_the_board_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("To Do", &["Plan"])]).await;
    let plan_card_id = card_id(&board, "Plan").to_owned();

    start_card(&db_pool, &signed_in_person, &plan_card_id).await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["started"]);
    assert_eq!(activity_entries[0]["title"], "Plan");
    assert_eq!(activity_entries[0]["contextName"], board["board"]["name"]);
    assert_eq!(activity_entries[0]["cardId"], plan_card_id.as_str());
    assert_eq!(activity_entries[0]["boardId"], board["board"]["id"]);
    assert_eq!(activity_entries[0]["details"]["subject"], "card");
}

#[sqlx::test]
async fn a_refused_start_adds_no_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    start_task(&db_pool, &signed_in_person, &task_id).await;

    let second_start = post_start(
        &db_pool,
        &signed_in_person,
        &format!("/tasks/{task_id}/start"),
    )
    .await;

    assert_eq!(second_start.status(), StatusCode::CONFLICT);
    assert_eq!(
        entry_kinds(&list_entries(&db_pool, &signed_in_person).await),
        vec!["started"]
    );
}

#[sqlx::test]
async fn checking_in_adds_a_checked_in_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tracking/{}/check-in", event_id(&running_item)),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let checked_in_item = response_json(response).await;
    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(
        entry_kinds(&activity_entries),
        vec!["checked_in", "started"]
    );
    assert_eq!(
        activity_entries[0]["occurredAt"],
        checked_in_item["lastAnsweredAt"]
    );
    assert_eq!(
        activity_entries[0]["details"]["eventId"],
        running_item["id"]
    );
}

#[sqlx::test]
async fn a_refused_check_in_adds_no_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;

    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tracking/{}/check-in", event_id(&running_item)),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        entry_kinds(&list_entries(&db_pool, &signed_in_person).await),
        vec!["finished", "started"]
    );
}

#[sqlx::test]
async fn finishing_adds_a_finished_entry_with_the_tracked_length(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["finished", "started"]);
    assert_eq!(
        activity_entries[0]["details"],
        json!({
            "subject": "task",
            "eventId": running_item["id"],
            "trackedMinutes": 0,
            "markedDone": false,
        })
    );
}

#[sqlx::test]
async fn finishing_at_an_earlier_end_notes_when_it_ended(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    std::thread::sleep(Duration::from_millis(20));
    sqlx::query(
        "UPDATE calendar_events SET starts_at = starts_at - interval '2 hours' WHERE id = $1::uuid",
    )
    .bind(event_id(&running_item))
    .execute(&db_pool)
    .await
    .expect("moving the start back should succeed");

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(-30) }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let finished_event = response_json(response).await["event"].clone();
    let finished_details = list_entries(&db_pool, &signed_in_person).await[0]["details"].clone();
    assert!(
        (89..=90).contains(
            &finished_details["trackedMinutes"]
                .as_i64()
                .expect("minutes")
        ),
        "{finished_details}"
    );
    assert_eq!(
        finished_details["endedAtMillis"].as_i64(),
        daibx_api::core::utc_instant::UtcInstant::parse(
            finished_event["endsAt"].as_str().expect("an end")
        )
        .map(|ends_at| ends_at.unix_millis())
    );
}

#[sqlx::test]
async fn finishing_with_mark_done_adds_one_finished_entry_and_no_tick(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0), "markDone": true }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["finished", "started"]);
    assert_eq!(activity_entries[0]["details"]["markedDone"], true);
}

#[sqlx::test]
async fn finishing_a_card_adds_a_finished_card_entry(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("To Do", &["Plan"])]).await;
    let running_item = start_card(&db_pool, &signed_in_person, card_id(&board, "Plan")).await;

    post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;

    let activity_entries = list_entries(&db_pool, &signed_in_person).await;
    assert_eq!(entry_kinds(&activity_entries), vec!["finished", "started"]);
    assert_eq!(activity_entries[0]["title"], "Plan");
    assert_eq!(activity_entries[0]["details"]["subject"], "card");
}
