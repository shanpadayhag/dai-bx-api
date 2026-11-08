use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tracking::running::list_running;
use crate::tracking::start::{add_top_level_task, start_task};

async fn post_check_in(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    event_id: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tracking/{event_id}/check-in"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn a_check_in_records_when_it_was_answered(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = post_check_in(&db_pool, &signed_in_person, event_id(&running_item)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let checked_in_item = response_json(response).await;
    let last_answered_at = checked_in_item["lastAnsweredAt"]
        .as_str()
        .expect("lastAnsweredAt should be set");
    assert!(last_answered_at >= running_item["startsAt"].as_str().expect("a start"));
    assert_eq!(checked_in_item["endsAt"], serde_json::Value::Null);
    assert_eq!(checked_in_item["title"], "Write the changelog");
    assert!(checked_in_item.get("estimateMinutes").is_some());
    let running_items = list_running(&db_pool, &signed_in_person).await;
    assert_eq!(running_items[0]["lastAnsweredAt"], last_answered_at);
}

#[sqlx::test]
async fn refuses_a_check_in_on_an_event_that_is_not_running(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00",
        "2026-11-05T10:00",
    )
    .await;

    let response = post_check_in(&db_pool, &signed_in_person, event_id(&gym_event)).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn answers_not_found_for_another_person_s_running_event(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &owner, "Write the changelog").await;
    let running_item = start_task(&db_pool, &owner, &task_id).await;

    for checked_event_id in [event_id(&running_item), "not-a-uuid"] {
        let response = post_check_in(&db_pool, &other_person, checked_event_id).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
