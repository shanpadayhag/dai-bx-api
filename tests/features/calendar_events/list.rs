use axum::http::StatusCode;
use sqlx::PgPool;

use crate::calendar_events::create::create_planned_event;
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, response_json, sign_up_with_authenticator,
};

async fn get_events(
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

fn event_titles(events_json: &serde_json::Value) -> Vec<&str> {
    events_json
        .as_array()
        .expect("a list")
        .iter()
        .map(|event_json| event_json["title"].as_str().expect("a title"))
        .collect()
}

#[sqlx::test]
async fn lists_events_overlapping_the_range_in_start_order(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for (title, starts_at, ends_at) in [
        (
            "Ends at the range start",
            "2026-11-01T22:00:00Z",
            "2026-11-02T00:00:00Z",
        ),
        (
            "Crosses the range start",
            "2026-11-01T23:00:00Z",
            "2026-11-02T01:00:00Z",
        ),
        ("Inside", "2026-11-04T09:00:00Z", "2026-11-04T10:00:00Z"),
        (
            "Crosses the range end",
            "2026-11-08T23:00:00Z",
            "2026-11-09T01:00:00Z",
        ),
        (
            "Starts at the range end",
            "2026-11-09T00:00:00Z",
            "2026-11-09T01:00:00Z",
        ),
        (
            "Covers the whole range",
            "2026-10-30T00:00:00Z",
            "2026-11-12T00:00:00Z",
        ),
    ] {
        create_planned_event(&db_pool, &signed_in_person, title, starts_at, ends_at).await;
    }

    let response = get_events(
        &db_pool,
        &signed_in_person,
        "from=2026-11-02T00:00:00Z&to=2026-11-09T00:00:00Z",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        event_titles(&response_json(response).await),
        vec![
            "Covers the whole range",
            "Crosses the range start",
            "Inside",
            "Crosses the range end",
        ]
    );
}

#[sqlx::test]
async fn lists_only_the_signed_in_person_s_events(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    create_planned_event(
        &db_pool,
        &other_person,
        "Theirs",
        "2026-11-04T09:00:00Z",
        "2026-11-04T10:00:00Z",
    )
    .await;
    create_planned_event(
        &db_pool,
        &signed_in_person,
        "Mine",
        "2026-11-04T09:00:00Z",
        "2026-11-04T10:00:00Z",
    )
    .await;

    let response = get_events(
        &db_pool,
        &signed_in_person,
        "from=2026-11-02T00:00:00Z&to=2026-11-09T00:00:00Z",
    )
    .await;

    assert_eq!(event_titles(&response_json(response).await), vec!["Mine"]);
}

#[sqlx::test]
async fn refuses_a_bad_or_too_long_range(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for (rule_description, query) in [
        (
            "over 42 days",
            "from=2026-11-01T00:00:00Z&to=2026-12-13T00:00:01Z",
        ),
        (
            "an end before the start",
            "from=2026-11-09T00:00:00Z&to=2026-11-02T00:00:00Z",
        ),
        ("a bad timestamp", "from=yesterday&to=2026-11-09T00:00:00Z"),
        ("a missing end", "from=2026-11-02T00:00:00Z"),
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
async fn allows_a_range_of_exactly_42_days(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_events(
        &db_pool,
        &signed_in_person,
        "from=2026-11-01T00:00:00Z&to=2026-12-13T00:00:00Z",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn listing_events_needs_a_session(db_pool: PgPool) {
    let response = get_with_cookies(
        build_test_app(db_pool),
        "/calendar/events?from=2026-11-02T00:00:00Z&to=2026-11-09T00:00:00Z",
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
