use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, post_event};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, response_json, sign_up_with_authenticator,
};

pub async fn get_events(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    query: &str,
) -> axum::response::Response {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/events?{query}"),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn list_events(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    query: &str,
) -> Vec<serde_json::Value> {
    let response = get_events(db_pool, signed_in_person, query).await;
    assert_eq!(response.status(), StatusCode::OK, "{query}");
    let calendar_range = response_json(response).await;
    calendar_range["events"]
        .as_array()
        .expect("a list of events")
        .clone()
}

fn event_titles(listed_events: &[serde_json::Value]) -> Vec<&str> {
    listed_events
        .iter()
        .map(|event_json| event_json["title"].as_str().expect("a title"))
        .collect()
}

#[sqlx::test]
async fn lists_events_overlapping_the_week_in_start_order(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for (title, start, end) in [
        (
            "Ends at the week start",
            "2026-11-01T22:00",
            "2026-11-02T00:00",
        ),
        (
            "Crosses the week start",
            "2026-11-01T23:00",
            "2026-11-02T01:00",
        ),
        ("Inside", "2026-11-04T09:00", "2026-11-04T10:00"),
        (
            "Crosses the week end",
            "2026-11-08T23:00",
            "2026-11-09T01:00",
        ),
        (
            "Starts at the week end",
            "2026-11-09T00:00",
            "2026-11-09T01:00",
        ),
        (
            "Covers the whole week",
            "2026-10-30T00:00",
            "2026-11-12T00:00",
        ),
    ] {
        create_planned_event(&db_pool, &signed_in_person, title, start, end).await;
    }

    let listed_events = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-02&days=7&timeZone=UTC",
    )
    .await;

    assert_eq!(
        event_titles(&listed_events),
        vec![
            "Covers the whole week",
            "Crosses the week start",
            "Inside",
            "Crosses the week end",
        ]
    );
}

#[sqlx::test]
async fn takes_the_day_from_midnight_to_midnight_in_the_asked_zone(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for (title, start, end) in [
        (
            "Late on the 4th in UTC",
            "2026-11-04T16:30",
            "2026-11-04T17:00",
        ),
        (
            "Early on the 5th in UTC",
            "2026-11-05T01:00",
            "2026-11-05T02:00",
        ),
        (
            "Late on the 5th in UTC",
            "2026-11-05T16:30",
            "2026-11-05T17:00",
        ),
    ] {
        create_planned_event(&db_pool, &signed_in_person, title, start, end).await;
    }

    let manila_day = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-05&days=1&timeZone=Asia/Manila",
    )
    .await;
    let utc_day = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-05&days=1&timeZone=UTC",
    )
    .await;

    assert_eq!(
        event_titles(&manila_day),
        vec!["Late on the 4th in UTC", "Early on the 5th in UTC"]
    );
    assert_eq!(
        event_titles(&utc_day),
        vec!["Early on the 5th in UTC", "Late on the 5th in UTC"]
    );
}

#[sqlx::test]
async fn takes_the_25_hours_of_the_day_the_clocks_go_back(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for (title, start, end) in [
        ("Saturday night", "2026-10-24T22:00", "2026-10-24T22:30"),
        (
            "Just after midnight",
            "2026-10-24T23:00",
            "2026-10-24T23:30",
        ),
        ("Last half hour", "2026-10-25T23:30", "2026-10-26T00:00"),
        ("Monday", "2026-10-26T00:00", "2026-10-26T00:30"),
    ] {
        create_planned_event(&db_pool, &signed_in_person, title, start, end).await;
    }

    let fall_back_day = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-10-25&days=1&timeZone=Europe/London",
    )
    .await;
    let fall_back_week = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-10-19&days=7&timeZone=Europe/London",
    )
    .await;

    assert_eq!(
        event_titles(&fall_back_day),
        vec!["Just after midnight", "Last half hour"]
    );
    assert_eq!(
        event_titles(&fall_back_week),
        vec!["Saturday night", "Just after midnight", "Last half hour"]
    );
}

#[sqlx::test]
async fn returns_an_event_typed_in_another_zone_at_the_right_instant(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Call home",
            "start": "2026-11-09T11:00",
            "end": "2026-11-09T11:30",
            "inputZone": "Europe/London",
            "timeZone": "Europe/London",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let manila_day = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-09&days=1&timeZone=Asia/Manila",
    )
    .await;

    assert_eq!(manila_day.len(), 1);
    assert_eq!(manila_day[0]["startsAt"], "2026-11-09T11:00:00.000Z");
    assert_eq!(manila_day[0]["timeZone"], "Europe/London");
}

#[sqlx::test]
async fn lists_only_the_signed_in_person_s_events(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    create_planned_event(
        &db_pool,
        &other_person,
        "Theirs",
        "2026-11-04T09:00",
        "2026-11-04T10:00",
    )
    .await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Mine",
        "2026-11-04T09:00",
        "2026-11-04T10:00",
    )
    .await;

    let listed_events = list_events(
        &db_pool,
        &signed_in_person,
        "start=2026-11-02&days=7&timeZone=UTC",
    )
    .await;

    assert_eq!(event_titles(&listed_events), vec!["Mine"]);
}

#[sqlx::test]
async fn refuses_a_bad_day_length_or_zone(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for (rule_description, query) in [
        ("three days", "start=2026-11-02&days=3&timeZone=UTC"),
        ("no day count", "start=2026-11-02&timeZone=UTC"),
        (
            "a day that doesn't exist",
            "start=2026-02-30&days=1&timeZone=UTC",
        ),
        (
            "a full timestamp",
            "start=2026-11-02T00:00:00Z&days=1&timeZone=UTC",
        ),
        (
            "an unknown zone",
            "start=2026-11-02&days=7&timeZone=Mars/Olympus",
        ),
        ("an offset", "start=2026-11-02&days=7&timeZone=%2B08:00"),
        ("a missing zone", "start=2026-11-02&days=7"),
        (
            "the old from and to",
            "from=2026-11-02T00:00:00Z&to=2026-11-09T00:00:00Z",
        ),
    ] {
        let response = get_events(&db_pool, &signed_in_person, query).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "should refuse {rule_description}"
        );
    }
}

#[sqlx::test]
async fn listing_events_needs_a_session(db_pool: PgPool) {
    let response = get_with_cookies(
        build_test_app(db_pool),
        "/calendar/events?start=2026-11-02&days=7&timeZone=UTC",
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
