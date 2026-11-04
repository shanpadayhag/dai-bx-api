use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id};
use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn patch_event(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    event_id: &str,
    request_body: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/events/{event_id}"),
        request_body,
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn changes_a_planned_event_s_title_category_and_times(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;

    let response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&gym_event),
        json!({
            "title": "Swim",
            "categoryId": studies_category_id,
            "startsAt": "2026-11-06T18:00:00Z",
            "endsAt": "2026-11-07T19:00:00Z",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_event = response_json(response).await;
    assert_eq!(updated_event["title"], "Swim");
    assert_eq!(updated_event["categoryId"], studies_category_id.as_str());
    assert_eq!(updated_event["startsAt"], "2026-11-06T18:00:00.000Z");
    assert_eq!(updated_event["endsAt"], "2026-11-07T19:00:00.000Z");
}

#[sqlx::test]
async fn leaves_missing_fields_alone_and_clears_a_null_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;
    patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&gym_event),
        json!({ "categoryId": studies_category_id }),
    )
    .await;

    let response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&gym_event),
        json!({ "categoryId": null, "endsAt": "2026-11-05T11:00:00Z" }),
    )
    .await;

    let updated_event = response_json(response).await;
    assert_eq!(updated_event["title"], "Gym");
    assert_eq!(updated_event["categoryId"], serde_json::Value::Null);
    assert_eq!(updated_event["startsAt"], "2026-11-05T09:00:00.000Z");
    assert_eq!(updated_event["endsAt"], "2026-11-05T11:00:00.000Z");
}

#[sqlx::test]
async fn refuses_changes_that_break_the_planned_rules(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;

    for (rule_description, rejected_body) in [
        ("a null title", json!({ "title": null })),
        (
            "a title over 200 characters",
            json!({ "title": "a".repeat(201) }),
        ),
        ("a bad timestamp", json!({ "startsAt": "05/11/2026 09:00" })),
        (
            "an end moved under 15 minutes",
            json!({ "endsAt": "2026-11-05T09:10:00Z" }),
        ),
        (
            "a start moved past the end",
            json!({ "startsAt": "2026-11-05T11:00:00Z" }),
        ),
        ("a null end", json!({ "endsAt": null })),
    ] {
        let response = patch_event(
            &db_pool,
            &signed_in_person,
            event_id(&gym_event),
            rejected_body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "should refuse {rule_description}"
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_event_or_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let their_event = create_planned_event(
        &db_pool,
        &other_person,
        "Theirs",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;
    let my_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Mine",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;
    let their_category_id = find_category_id(&db_pool, &other_person, "Studies").await;

    let their_event_response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&their_event),
        json!({ "title": "Taken" }),
    )
    .await;
    let their_category_response = patch_event(
        &db_pool,
        &signed_in_person,
        event_id(&my_event),
        json!({ "categoryId": their_category_id }),
    )
    .await;
    let unknown_event_response = patch_event(
        &db_pool,
        &signed_in_person,
        "not-an-id",
        json!({ "title": "Taken" }),
    )
    .await;

    assert_eq!(their_event_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(their_category_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(unknown_event_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn changing_an_event_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/calendar/events/{}", event_id(&gym_event)),
        json!({ "title": "Swim" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
