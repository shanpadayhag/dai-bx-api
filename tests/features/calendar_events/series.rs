use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id, post_event};
use crate::calendar_events::list::list_events;
use crate::calendar_events::update::patch_event;
use crate::support::{SignedInPerson, response_json, sign_up_with_authenticator};
use crate::tracking::start::{add_top_level_task, start_task};

fn weekly_on(weekdays: &[&str], ends: serde_json::Value) -> serde_json::Value {
    json!({ "frequency": "weekly", "interval": 1, "weekdays": weekdays, "ends": ends })
}

async fn create_series(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: serde_json::Value,
) -> serde_json::Value {
    let response = post_event(db_pool, signed_in_person, request_body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

fn starts(listed_events: &[serde_json::Value]) -> Vec<&str> {
    listed_events
        .iter()
        .map(|event_json| event_json["startsAt"].as_str().expect("a start"))
        .collect()
}

#[sqlx::test]
async fn creates_a_weekly_series_that_shows_on_every_week_asked_for(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let weekly_on_monday = weekly_on(&["mon"], json!({ "never": true }));

    let first_occurrence = create_series(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Gym",
            "start": "2026-11-09T18:00",
            "end": "2026-11-09T19:00",
            "inputZone": "Asia/Manila",
            "repeat": weekly_on_monday,
        }),
    )
    .await;

    let series_id = first_occurrence["series"]["id"]
        .as_str()
        .expect("a series id");
    assert_eq!(
        first_occurrence["id"],
        format!("{series_id}:2026-11-09T10:00:00.000Z")
    );
    assert_eq!(first_occurrence["kind"], "planned");
    assert_eq!(first_occurrence["title"], "Gym");
    assert_eq!(first_occurrence["startsAt"], "2026-11-09T10:00:00.000Z");
    assert_eq!(first_occurrence["endsAt"], "2026-11-09T11:00:00.000Z");
    assert_eq!(first_occurrence["timeZone"], "Asia/Manila");
    assert_eq!(
        first_occurrence["series"]["originalStart"],
        "2026-11-09T10:00:00.000Z"
    );
    assert_eq!(first_occurrence["series"]["repeat"], weekly_on_monday);
    for (week_start, expected_start) in [
        ("2026-11-09", "2026-11-09T10:00:00.000Z"),
        ("2026-12-07", "2026-12-07T10:00:00.000Z"),
        ("2027-11-08", "2027-11-08T10:00:00.000Z"),
    ] {
        let listed_events = list_events(
            &db_pool,
            &signed_in_person,
            &format!("start={week_start}&days=7&timeZone=Asia/Manila"),
        )
        .await;
        assert_eq!(starts(&listed_events), vec![expected_start], "{week_start}");
        assert_eq!(
            listed_events[0]["id"],
            format!("{series_id}:{expected_start}")
        );
        assert_eq!(listed_events[0]["series"]["originalStart"], expected_start);
    }
    let week_before = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-02&days=7&timeZone=Asia/Manila",
    )
    .await;
    assert!(week_before.is_empty());
}

#[sqlx::test]
async fn stops_a_series_on_its_end_date_or_after_its_count(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    create_series(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Until the 23rd",
            "start": "2026-11-09T09:00",
            "end": "2026-11-09T10:00",
            "inputZone": "UTC",
            "repeat": weekly_on(&["mon"], json!({ "onDate": "2026-11-23" })),
        }),
    )
    .await;
    create_series(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Three times",
            "start": "2026-11-24T12:00",
            "end": "2026-11-24T12:30",
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "after": 3 } },
        }),
    )
    .await;

    let last_week = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-23&days=7&timeZone=UTC",
    )
    .await;
    let week_after = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-30&days=7&timeZone=UTC",
    )
    .await;

    assert_eq!(
        starts(&last_week),
        vec![
            "2026-11-23T09:00:00.000Z",
            "2026-11-24T12:00:00.000Z",
            "2026-11-25T12:00:00.000Z",
            "2026-11-26T12:00:00.000Z",
        ]
    );
    assert!(week_after.is_empty());
}

#[sqlx::test]
async fn keeps_a_series_wall_clock_time_in_its_own_zone_across_daylight_saving(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let first_occurrence = create_series(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Call home",
            "start": "2026-11-09T19:00",
            "end": "2026-11-09T19:30",
            "inputZone": "Asia/Manila",
            "timeZone": "Europe/London",
            "repeat": weekly_on(&["mon"], json!({ "never": true })),
        }),
    )
    .await;

    let summer_week = list_events(
        &db_pool,
        &signed_in_person,
        "start=2027-04-05&days=7&timeZone=Asia/Manila",
    )
    .await;

    assert_eq!(first_occurrence["startsAt"], "2026-11-09T11:00:00.000Z");
    assert_eq!(first_occurrence["timeZone"], "Europe/London");
    assert_eq!(starts(&summer_week), vec!["2027-04-05T10:00:00.000Z"]);
    assert_eq!(summer_week[0]["endsAt"], "2027-04-05T10:30:00.000Z");
}

#[sqlx::test]
async fn starts_a_series_on_its_first_repeat_day(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let first_occurrence = create_series(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Gym",
            "start": "2026-11-11T09:00",
            "end": "2026-11-11T10:00",
            "inputZone": "UTC",
            "repeat": weekly_on(&["mon"], json!({ "never": true })),
        }),
    )
    .await;

    assert_eq!(first_occurrence["startsAt"], "2026-11-16T09:00:00.000Z");
}

#[sqlx::test]
async fn refuses_a_bad_repeat_or_one_that_never_happens(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for (rule_description, refused_repeat) in [
        (
            "an interval of 0",
            json!({ "frequency": "daily", "interval": 0, "ends": { "never": true } }),
        ),
        (
            "a weekly repeat without weekdays",
            json!({ "frequency": "weekly", "interval": 1, "ends": { "never": true } }),
        ),
        (
            "an unknown frequency",
            json!({ "frequency": "hourly", "interval": 1, "ends": { "never": true } }),
        ),
        (
            "an end before the start",
            weekly_on(&["mon"], json!({ "onDate": "2026-11-01" })),
        ),
    ] {
        let response = post_event(
            &db_pool,
            &signed_in_person,
            json!({
                "title": "Gym",
                "start": "2026-11-09T09:00",
                "end": "2026-11-09T10:00",
                "inputZone": "UTC",
                "repeat": refused_repeat,
            }),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "should refuse {rule_description}"
        );
    }
    let week = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-09&days=7&timeZone=UTC",
    )
    .await;
    assert!(week.is_empty());
}

#[sqlx::test]
async fn turns_an_event_into_a_series_when_a_repeat_is_added(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-09T09:00",
        "2026-11-09T10:00",
    )
    .await;

    let response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&gym_event),
        json!({
            "title": "Swim",
            "inputZone": "UTC",
            "repeat": weekly_on(&["mon", "thu"], json!({ "never": true })),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let first_occurrence = response_json(response).await;
    assert_eq!(first_occurrence["title"], "Swim");
    assert_eq!(first_occurrence["startsAt"], "2026-11-09T09:00:00.000Z");
    assert!(first_occurrence["series"]["id"].is_string());
    let week = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-09&days=7&timeZone=UTC",
    )
    .await;
    assert_eq!(
        starts(&week),
        vec!["2026-11-09T09:00:00.000Z", "2026-11-12T09:00:00.000Z"]
    );
    assert!(
        week.iter()
            .all(|listed_event| listed_event["title"] == "Swim")
    );
    let old_event_response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&gym_event),
        json!({ "title": "Run" }),
    )
    .await;
    assert_eq!(old_event_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn answers_not_found_when_adding_a_repeat_to_someone_else_s_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let their_event = create_planned_event(
        &db_pool,
        &other_person,
        "Theirs",
        "2026-11-09T09:00",
        "2026-11-09T10:00",
    )
    .await;

    let response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&their_event),
        json!({
            "inputZone": "UTC",
            "repeat": weekly_on(&["mon"], json!({ "never": true })),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let their_week = list_events(
        &db_pool,
        &other_person,
        "start=2026-11-09&days=7&timeZone=UTC",
    )
    .await;
    assert_eq!(their_week.len(), 1);
    assert_eq!(their_week[0]["series"], serde_json::Value::Null);
}

#[sqlx::test]
async fn refuses_a_repeat_or_a_zone_on_a_tracked_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    for refused_change in [
        json!({
            "inputZone": "UTC",
            "repeat": { "frequency": "daily", "interval": 1, "ends": { "never": true } },
        }),
        json!({ "timeZone": "Europe/London" }),
    ] {
        let response = patch_event(
            &db_pool,
            &signed_in_person,
            event_id(&running_item),
            refused_change.clone(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_change}"
        );
    }
}
