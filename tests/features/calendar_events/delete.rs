use axum::http::StatusCode;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id};
use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_planned_event(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/calendar/events/{}", event_id(&gym_event)),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "deleted");
    let listed_events = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/calendar/events?from=2026-11-02T00:00:00Z&to=2026-11-09T00:00:00Z",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    assert_eq!(listed_events, serde_json::json!([]));
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_an_already_deleted_event(db_pool: PgPool) {
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
    let my_event_path = format!("/calendar/events/{}", event_id(&my_event));
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &my_event_path,
        &signed_in_person.session_cookies,
    )
    .await;

    for event_path in [
        format!("/calendar/events/{}", event_id(&their_event)),
        my_event_path,
    ] {
        let response = delete_with_cookies(
            build_test_app(db_pool.clone()),
            &event_path,
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{event_path}");
    }
}

#[sqlx::test]
async fn deleting_an_event_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/calendar/events/{}", event_id(&gym_event)),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
