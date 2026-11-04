use std::time::Duration;

use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::event_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, find_task_named};
use crate::tracking::closing::find_calendar_event;
use crate::tracking::finish::post_finish;
use crate::tracking::start::{add_top_level_task, start_card, start_task};

async fn finished_then_running(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    start_item: impl AsyncFn() -> serde_json::Value,
) -> (String, String) {
    let finished_item = start_item().await;
    std::thread::sleep(Duration::from_millis(20));
    let finish_response = post_finish(
        db_pool,
        signed_in_person,
        event_id(&finished_item),
        json!({ "endsAt": UtcInstant::now().plus_minutes(1).to_iso_text() }),
    )
    .await;
    assert_eq!(finish_response.status(), StatusCode::OK);
    let running_item = start_item().await;
    (
        event_id(&finished_item).to_owned(),
        event_id(&running_item).to_owned(),
    )
}

async fn patch_path(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_path: &str,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        request_path,
        request_body,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "{request_path}");
    response_json(response).await
}

async fn event_titles(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    event_ids: &[&str],
) -> Vec<String> {
    let mut titles = Vec::new();
    for wanted_event_id in event_ids {
        let calendar_event = find_calendar_event(db_pool, signed_in_person, wanted_event_id).await;
        titles.push(
            calendar_event["title"]
                .as_str()
                .expect("a title")
                .to_owned(),
        );
    }
    titles
}

async fn card_detail(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    wanted_card_id: &str,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{wanted_card_id}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn renaming_a_task_with_rename_events_renames_every_tracked_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let (finished_event_id, running_event_id) =
        finished_then_running(&db_pool, &signed_in_person, async || {
            start_task(&db_pool, &signed_in_person, &task_id).await
        })
        .await;

    let group_tasks = patch_path(
        &db_pool,
        &signed_in_person,
        &format!("/tasks/{task_id}"),
        json!({ "name": "Write the release notes", "renameEvents": true }),
    )
    .await;

    assert_eq!(
        find_task_named(&group_tasks, "Write the release notes")["trackedEventCount"],
        2
    );
    assert_eq!(
        event_titles(
            &db_pool,
            &signed_in_person,
            &[&finished_event_id, &running_event_id]
        )
        .await,
        vec!["Write the release notes", "Write the release notes"]
    );
}

#[sqlx::test]
async fn renaming_a_task_without_rename_events_keeps_the_old_names(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let (finished_event_id, running_event_id) =
        finished_then_running(&db_pool, &signed_in_person, async || {
            start_task(&db_pool, &signed_in_person, &task_id).await
        })
        .await;

    for rename_request in [
        json!({ "name": "Write the release notes", "renameEvents": false }),
        json!({ "name": "Draft the release notes" }),
    ] {
        patch_path(
            &db_pool,
            &signed_in_person,
            &format!("/tasks/{task_id}"),
            rename_request,
        )
        .await;
    }

    assert_eq!(
        event_titles(
            &db_pool,
            &signed_in_person,
            &[&finished_event_id, &running_event_id]
        )
        .await,
        vec!["Write the changelog", "Write the changelog"]
    );
}

#[sqlx::test]
async fn counts_tracked_events_per_task(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    finished_then_running(&db_pool, &signed_in_person, async || {
        start_task(&db_pool, &signed_in_person, &task_id).await
    })
    .await;
    add_task(&db_pool, &signed_in_person, &task_group_id, "Review", None).await;

    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Collect the commits",
        Some(&task_id),
    )
    .await;

    for (task_name, tracked_event_count) in [
        ("Write the changelog", 2),
        ("Review", 0),
        ("Collect the commits", 0),
    ] {
        assert_eq!(
            find_task_named(&group_tasks, task_name)["trackedEventCount"],
            tracked_event_count,
            "{task_name}"
        );
    }
}

#[sqlx::test]
async fn renaming_a_card_renames_its_events_only_when_asked(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release", "Book the venue"])],
    )
    .await;
    let release_card_id = card_id(&board, "Plan the release").to_owned();
    let (finished_event_id, running_event_id) =
        finished_then_running(&db_pool, &signed_in_person, async || {
            start_card(&db_pool, &signed_in_person, &release_card_id).await
        })
        .await;

    assert_eq!(
        card_detail(&db_pool, &signed_in_person, &release_card_id).await["trackedEventCount"],
        2
    );
    assert_eq!(
        card_detail(
            &db_pool,
            &signed_in_person,
            card_id(&board, "Book the venue")
        )
        .await["trackedEventCount"],
        0
    );

    patch_path(
        &db_pool,
        &signed_in_person,
        &format!("/cards/{release_card_id}"),
        json!({ "title": "Plan the launch" }),
    )
    .await;
    assert_eq!(
        event_titles(
            &db_pool,
            &signed_in_person,
            &[&finished_event_id, &running_event_id]
        )
        .await,
        vec!["Plan the release", "Plan the release"]
    );

    patch_path(
        &db_pool,
        &signed_in_person,
        &format!("/cards/{release_card_id}"),
        json!({ "title": "Plan the launch party", "renameEvents": true }),
    )
    .await;
    assert_eq!(
        event_titles(
            &db_pool,
            &signed_in_person,
            &[&finished_event_id, &running_event_id]
        )
        .await,
        vec!["Plan the launch party", "Plan the launch party"]
    );
}
