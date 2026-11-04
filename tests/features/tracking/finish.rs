use std::time::Duration;

use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::event_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, find_task_named, task_id_named};
use crate::tracking::running::list_running;
use crate::tracking::start::{add_top_level_task, start_card, start_task};

pub async fn post_finish(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    event_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tracking/{event_id}/finish"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

fn instant_of(instant_json: &serde_json::Value) -> UtcInstant {
    UtcInstant::parse(instant_json.as_str().expect("a time")).expect("a valid time")
}

fn minutes_from_now(minute_count: i64) -> String {
    UtcInstant::now().plus_minutes(minute_count).to_iso_text()
}

#[sqlx::test]
async fn a_finish_a_moment_ahead_ends_the_event_at_now(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(1) }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let finish_result = response_json(response).await;
    let finished_event = &finish_result["event"];
    assert_eq!(finished_event["id"], running_item["id"]);
    assert_eq!(finished_event["endsAt"], finished_event["finishedAt"]);
    assert!(instant_of(&finished_event["endsAt"]) <= UtcInstant::now());
    assert!(instant_of(&finished_event["endsAt"]) > instant_of(&running_item["startsAt"]));
    assert!(finish_result.get("groupTasks").is_none());
    assert_eq!(list_running(&db_pool, &signed_in_person).await, json!([]));
}

#[sqlx::test]
async fn a_finish_at_the_last_check_in_keeps_the_rest_as_trimmed_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    std::thread::sleep(Duration::from_millis(20));
    let check_in_response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tracking/{}/check-in", event_id(&running_item)),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await;
    let last_answered_at = response_json(check_in_response).await["lastAnsweredAt"].clone();
    std::thread::sleep(Duration::from_millis(20));

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": last_answered_at }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let finished_event = response_json(response).await["event"].clone();
    assert_eq!(finished_event["endsAt"], last_answered_at);
    assert!(instant_of(&finished_event["finishedAt"]) > instant_of(&finished_event["endsAt"]));
}

#[sqlx::test]
async fn a_finish_at_a_chosen_time_keeps_that_time(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    std::thread::sleep(Duration::from_millis(20));
    let chosen_ends_at =
        UtcInstant::from_unix_millis(instant_of(&running_item["startsAt"]).unix_millis() + 10)
            .to_iso_text();

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": chosen_ends_at }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let finished_event = response_json(response).await["event"].clone();
    assert_eq!(finished_event["endsAt"], chosen_ends_at.as_str());
}

#[sqlx::test]
async fn refuses_an_end_outside_the_start_and_now(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let starts_at = running_item["startsAt"]
        .as_str()
        .expect("a start")
        .to_owned();

    for refused_body in [
        json!({ "endsAt": starts_at }),
        json!({ "endsAt": "2025-11-05T09:00:00Z" }),
        json!({ "endsAt": minutes_from_now(3) }),
        json!({ "endsAt": "tomorrow" }),
        json!({}),
    ] {
        let response = post_finish(
            &db_pool,
            &signed_in_person,
            event_id(&running_item),
            refused_body.clone(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_body}"
        );
    }
    assert_eq!(
        list_running(&db_pool, &signed_in_person)
            .await
            .as_array()
            .expect("a list")
            .len(),
        1
    );
}

#[sqlx::test]
async fn refuses_to_finish_an_event_twice(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let first_response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0) }),
    )
    .await;

    assert_eq!(second_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn marking_it_done_completes_the_task_and_its_subtasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Release", None).await;
    let release_task_id = task_id_named(&group_tasks, "Release");
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Tag the build",
        Some(&release_task_id),
    )
    .await;
    let running_item = start_task(&db_pool, &signed_in_person, &release_task_id).await;

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0), "markDone": true }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let finish_result = response_json(response).await;
    assert!(finish_result["event"]["endsAt"].is_string());
    let group_tasks = &finish_result["groupTasks"];
    assert_eq!(group_tasks["group"]["id"], task_group_id.as_str());
    assert_eq!(find_task_named(group_tasks, "Release")["isDone"], true);
    assert_eq!(
        find_task_named(group_tasks, "Tag the build")["isDone"],
        true
    );
}

#[sqlx::test]
async fn refuses_to_mark_a_card_done_and_keeps_it_running(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;
    let running_item = start_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the release"),
    )
    .await;

    let response = post_finish(
        &db_pool,
        &signed_in_person,
        event_id(&running_item),
        json!({ "endsAt": minutes_from_now(0), "markDone": true }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        list_running(&db_pool, &signed_in_person).await[0]["id"],
        running_item["id"]
    );
}

#[sqlx::test]
async fn answers_not_found_for_another_person_s_event(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &owner, "Write the changelog").await;
    let running_item = start_task(&db_pool, &owner, &task_id).await;

    for finished_event_id in [event_id(&running_item), "not-a-uuid"] {
        for mark_done in [false, true] {
            let response = post_finish(
                &db_pool,
                &other_person,
                finished_event_id,
                json!({ "endsAt": minutes_from_now(0), "markDone": mark_done }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
}
