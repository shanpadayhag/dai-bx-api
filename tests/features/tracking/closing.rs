use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::list_id;
use crate::boards::create::board_id;
use crate::calendar_events::create::event_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};
use crate::tasks::moving::move_task;
use crate::tracking::running::list_running;
use crate::tracking::start::{add_top_level_task, start_card, start_task};

pub async fn find_calendar_event(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    wanted_event_id: &str,
) -> serde_json::Value {
    let from = UtcInstant::now().plus_days(-1).to_iso_text();
    let to = UtcInstant::now().plus_days(1).to_iso_text();
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/events?from={from}&to={to}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response)
        .await
        .as_array()
        .expect("a list of events")
        .iter()
        .find(|calendar_event| calendar_event["id"] == wanted_event_id)
        .expect("the event should still be on the calendar")
        .clone()
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

fn assert_closed_at_now(closed_event: &serde_json::Value, title: &str) {
    assert_eq!(closed_event["title"], title);
    assert!(closed_event["endsAt"].is_string());
    assert_eq!(closed_event["endsAt"], closed_event["finishedAt"]);
}

async fn running_ids(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> Vec<String> {
    list_running(db_pool, signed_in_person)
        .await
        .as_array()
        .expect("a list")
        .iter()
        .map(|running_item| running_item["id"].as_str().expect("an id").to_owned())
        .collect()
}

#[sqlx::test]
async fn completing_a_running_task_closes_its_event_at_now(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let (other_task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Review").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let other_running_item = start_task(&db_pool, &signed_in_person, &other_task_id).await;

    set_task_done(&db_pool, &signed_in_person, &task_id, true).await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Write the changelog");
    assert_eq!(closed_event["source"]["id"], task_id.as_str());
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn deleting_a_running_task_closes_its_event_and_drops_the_link(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    delete_path(&db_pool, &signed_in_person, &format!("/tasks/{task_id}")).await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Write the changelog");
    assert_eq!(closed_event["source"], serde_json::Value::Null);
    assert!(running_ids(&db_pool, &signed_in_person).await.is_empty());
}

#[sqlx::test]
async fn deleting_a_group_closes_its_running_tasks_only(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let (other_task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Review").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let other_running_item = start_task(&db_pool, &signed_in_person, &other_task_id).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/task-groups/{task_group_id}"),
    )
    .await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Write the changelog");
    assert_eq!(closed_event["source"], serde_json::Value::Null);
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn deleting_a_running_card_closes_its_event_and_drops_the_link(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release", "Write notes"])],
    )
    .await;
    let release_card_id = card_id(&board, "Plan the release");
    let running_item = start_card(&db_pool, &signed_in_person, release_card_id).await;
    let other_running_item =
        start_card(&db_pool, &signed_in_person, card_id(&board, "Write notes")).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/cards/{release_card_id}"),
    )
    .await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Plan the release");
    assert_eq!(closed_event["source"], serde_json::Value::Null);
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn deleting_a_list_closes_its_running_cards_only(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[
            ("Doing", &["Plan the release"]),
            ("Later", &["Write notes"]),
        ],
    )
    .await;
    let running_item = start_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the release"),
    )
    .await;
    let other_running_item =
        start_card(&db_pool, &signed_in_person, card_id(&board, "Write notes")).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/lists/{}", list_id(&board, "Doing")),
    )
    .await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Plan the release");
    assert_eq!(closed_event["source"], serde_json::Value::Null);
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn deleting_a_board_closes_its_running_cards_only(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Review").await;
    let running_item = start_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the release"),
    )
    .await;
    let other_running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    delete_path(
        &db_pool,
        &signed_in_person,
        &format!("/boards/{}", board_id(&board)),
    )
    .await;

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Plan the release");
    assert_eq!(closed_event["source"], serde_json::Value::Null);
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn moving_a_running_task_under_another_closes_its_event_at_now(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Release", None).await;
    let parent_task_id = task_id_named(&group_tasks, "Release");
    let (other_task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Review").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let other_running_item = start_task(&db_pool, &signed_in_person, &other_task_id).await;

    let response = move_task(
        &db_pool,
        &signed_in_person,
        &task_id,
        json!({ "groupId": task_group_id, "parentId": parent_task_id, "position": 0 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let closed_event =
        find_calendar_event(&db_pool, &signed_in_person, event_id(&running_item)).await;
    assert_closed_at_now(&closed_event, "Write the changelog");
    assert_eq!(closed_event["source"]["id"], task_id.as_str());
    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&other_running_item)]
    );
}

#[sqlx::test]
async fn moving_a_running_task_to_another_top_level_spot_keeps_it_running(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    add_task(&db_pool, &signed_in_person, &task_group_id, "Release", None).await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = move_task(
        &db_pool,
        &signed_in_person,
        &task_id,
        json!({ "groupId": task_group_id, "parentId": null, "position": 1 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        running_ids(&db_pool, &signed_in_person).await,
        vec![event_id(&running_item)]
    );
}
