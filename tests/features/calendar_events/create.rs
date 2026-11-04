use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json, send,
    sign_up_with_authenticator,
};

pub async fn post_event(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    request_body: serde_json::Value,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/calendar/events",
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn create_planned_event(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    title: &str,
    starts_at: &str,
    ends_at: &str,
) -> serde_json::Value {
    let response = post_event(
        db_pool,
        signed_in_person,
        json!({ "title": title, "startsAt": starts_at, "endsAt": ends_at }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

pub fn event_id(event_json: &serde_json::Value) -> &str {
    event_json["id"].as_str().expect("an event id")
}

#[sqlx::test]
async fn creates_a_planned_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;

    let response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "  Gym ",
            "categoryId": studies_category_id,
            "startsAt": "2026-11-05T09:00:00Z",
            "endsAt": "2026-11-05T10:30:00.000Z",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created_event = response_json(response).await;
    assert!(created_event["id"].is_string());
    assert_eq!(created_event["kind"], "planned");
    assert_eq!(created_event["title"], "Gym");
    assert_eq!(created_event["categoryId"], studies_category_id.as_str());
    assert_eq!(created_event["startsAt"], "2026-11-05T09:00:00.000Z");
    assert_eq!(created_event["endsAt"], "2026-11-05T10:30:00.000Z");
    assert_eq!(created_event["finishedAt"], serde_json::Value::Null);
    assert_eq!(created_event["lastAnsweredAt"], serde_json::Value::Null);
    assert_eq!(created_event["source"], serde_json::Value::Null);
}

#[sqlx::test]
async fn keeps_an_event_that_lasts_several_days(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let created_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Trip",
        "2026-11-05T09:00:00Z",
        "2026-11-08T15:00:00Z",
    )
    .await;

    assert_eq!(created_event["endsAt"], "2026-11-08T15:00:00.000Z");
}

#[sqlx::test]
async fn refuses_bad_titles_and_times(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let rejected_bodies = [
        (
            "an empty title",
            json!({ "title": "   ", "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T10:00:00Z" }),
        ),
        (
            "a title over 200 characters",
            json!({ "title": "a".repeat(201), "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T10:00:00Z" }),
        ),
        (
            "a time without a zone",
            json!({ "title": "Gym", "startsAt": "2026-11-05T09:00:00", "endsAt": "2026-11-05T10:00:00Z" }),
        ),
        (
            "a day that doesn't exist",
            json!({ "title": "Gym", "startsAt": "2026-02-30T09:00:00Z", "endsAt": "2026-03-01T10:00:00Z" }),
        ),
        (
            "an event under 15 minutes",
            json!({ "title": "Gym", "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T09:14:59Z" }),
        ),
        (
            "an end before the start",
            json!({ "title": "Gym", "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T08:00:00Z" }),
        ),
    ];

    for (rule_description, rejected_body) in rejected_bodies {
        let response = post_event(&db_pool, &signed_in_person, rejected_body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "should refuse {rule_description}"
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let other_category_id = find_category_id(&db_pool, &other_person, "Studies").await;

    let response = post_event(
        &db_pool,
        &signed_in_person,
        json!({
            "title": "Gym",
            "categoryId": other_category_id,
            "startsAt": "2026-11-05T09:00:00Z",
            "endsAt": "2026-11-05T10:00:00Z",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn creating_an_event_needs_a_session(db_pool: PgPool) {
    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/calendar/events",
        json!({ "title": "Gym", "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T10:00:00Z" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refuses_a_cross_site_event_write(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let cross_site_request = Request::post("/calendar/events")
        .header("sec-fetch-site", "cross-site")
        .header("content-type", "application/json")
        .header(header::COOKIE, &signed_in_person.session_cookies)
        .body(Body::from(
            json!({ "title": "Gym", "startsAt": "2026-11-05T09:00:00Z", "endsAt": "2026-11-05T10:00:00Z" })
                .to_string(),
        ))
        .expect("test request should build");

    let response = send(build_test_app(db_pool), cross_site_request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
