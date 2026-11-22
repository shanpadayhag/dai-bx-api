use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, post_event};
use crate::cards::archive::archive_card;
use crate::cards::create::{board_with_cards, card_id};
use crate::search::{listed_values, search_for};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tracking::start::{add_top_level_task, start_task};

async fn write_card_notes(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    card_notes: &str,
) {
    let response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}"),
        json!({ "notes": card_notes }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn create_series(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: serde_json::Value,
) -> String {
    let response = post_event(db_pool, signed_in_person, request_body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["series"]["id"]
        .as_str()
        .expect("a series id")
        .to_owned()
}

fn daily_from(start: &str, end: &str, title: &str, ends: serde_json::Value) -> serde_json::Value {
    json!({
        "title": title,
        "start": start,
        "end": end,
        "inputZone": "UTC",
        "repeat": { "frequency": "daily", "interval": 1, "ends": ends },
    })
}

#[sqlx::test]
async fn finds_cards_by_title_or_notes_with_a_snippet_only_for_a_notes_match(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("To do", &["Vocabulary drills", "Chinese lessons"])],
    )
    .await;
    write_card_notes(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Chinese lessons"),
        "Start with tones, then review the vocabulary list from HSK 3 before the weekend test.",
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=vocab").await;

    assert_eq!(
        listed_values(&search_results, "cards", "title"),
        vec!["Vocabulary drills", "Chinese lessons"]
    );
    let title_match = &search_results["cards"]["items"][0];
    assert_eq!(title_match["snippet"], serde_json::Value::Null);
    assert_eq!(title_match["boardName"], "Personal");
    assert_eq!(title_match["listName"], "To do");
    let notes_snippet = search_results["cards"]["items"][1]["snippet"]
        .as_str()
        .expect("a snippet for the notes match");
    assert!(notes_snippet.contains("vocabulary"));
    assert!(!notes_snippet.contains('<'));
}

#[sqlx::test]
async fn lists_archived_cards_after_live_ones_with_their_old_column(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the trip"]), ("Done", &["Plan the party"])],
    )
    .await;
    archive_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the trip"),
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    assert_eq!(
        listed_values(&search_results, "cards", "title"),
        vec!["Plan the party", "Plan the trip"]
    );
    let archived_match = &search_results["cards"]["items"][1];
    assert_eq!(archived_match["archived"], true);
    assert_eq!(archived_match["listName"], "Doing");
    assert_eq!(search_results["cards"]["items"][0]["archived"], false);
}

#[sqlx::test]
async fn lists_upcoming_planned_events_before_past_ones_and_never_tracked_ones(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym old",
        "2020-01-01T09:00",
        "2020-01-01T10:00",
    )
    .await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym later",
        "2031-01-01T09:00",
        "2031-01-01T10:00",
    )
    .await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym soon",
        "2030-01-01T09:00",
        "2030-01-01T10:00",
    )
    .await;
    let (gym_task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Gym bag").await;
    start_task(&db_pool, &signed_in_person, &gym_task_id).await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=gym&kind=events").await;

    assert_eq!(
        listed_values(&search_results, "events", "title"),
        vec!["Gym soon", "Gym later", "Gym old"]
    );
    let soon_event = &search_results["events"]["items"][0];
    assert_eq!(soon_event["startsAt"], "2030-01-01T09:00:00.000Z");
    assert_eq!(soon_event["repeats"], false);
    assert_eq!(soon_event["seriesId"], serde_json::Value::Null);
    assert_eq!(soon_event["timeZone"], serde_json::Value::Null);
}

#[sqlx::test]
async fn shows_a_repeating_event_once_at_its_next_occurrence_skipping_deleted_ones(
    db_pool: PgPool,
) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let series_id = create_series(
        &db_pool,
        &signed_in_person,
        daily_from(
            "2030-01-01T09:00",
            "2030-01-01T10:00",
            "Morning run",
            json!({ "never": true }),
        ),
    )
    .await;
    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/series/{series_id}/occurrences/2030-01-01T09:00:00.000Z?scope=this"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::OK);

    let search_results = search_for(&db_pool, &signed_in_person, "q=run&kind=events").await;

    assert_eq!(
        listed_values(&search_results, "events", "title"),
        vec!["Morning run"]
    );
    let series_match = &search_results["events"]["items"][0];
    assert_eq!(series_match["id"], series_id);
    assert_eq!(series_match["seriesId"], series_id);
    assert_eq!(series_match["repeats"], true);
    assert_eq!(series_match["startsAt"], "2030-01-02T09:00:00.000Z");
    assert_eq!(series_match["endsAt"], "2030-01-02T10:00:00.000Z");
}

#[sqlx::test]
async fn shows_an_ended_repeating_event_at_its_last_occurrence(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    create_series(
        &db_pool,
        &signed_in_person,
        daily_from(
            "2020-01-01T09:00",
            "2020-01-01T10:00",
            "Old standup",
            json!({ "after": 3 }),
        ),
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=standup&kind=events").await;

    assert_eq!(
        search_results["events"]["items"][0]["startsAt"],
        "2020-01-03T09:00:00.000Z"
    );
}

#[sqlx::test]
async fn gives_a_planned_block_its_linked_task_until_the_task_is_done(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (linked_task_id, _) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the report").await;
    let response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Report block",
            "start": "2030-01-01T09:00",
            "end": "2030-01-01T10:00",
            "inputZone": "UTC",
            "taskId": linked_task_id,
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let linked_results = search_for(&db_pool, &signed_in_person, "q=report&kind=events").await;
    set_task_done(&db_pool, &signed_in_person, &linked_task_id, true).await;
    let done_results = search_for(&db_pool, &signed_in_person, "q=report&kind=events").await;

    assert_eq!(
        linked_results["events"]["items"][0]["taskId"],
        linked_task_id
    );
    assert_eq!(
        done_results["events"]["items"][0]["taskId"],
        serde_json::Value::Null
    );
}

#[sqlx::test]
async fn never_finds_another_persons_cards_or_events(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    board_with_cards(&db_pool, &other_person, &[("To do", &["Plan the trip"])]).await;
    create_planned_event(
        &db_pool,
        &other_person,
        "Plan review",
        "2030-01-01T09:00",
        "2030-01-01T10:00",
    )
    .await;
    create_series(
        &db_pool,
        &other_person,
        daily_from(
            "2030-01-01T09:00",
            "2030-01-01T10:00",
            "Plan daily",
            json!({ "never": true }),
        ),
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    assert!(listed_values(&search_results, "cards", "title").is_empty());
    assert!(listed_values(&search_results, "events", "title").is_empty());
}
