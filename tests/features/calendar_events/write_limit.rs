use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::{create_planned_event, event_id};
use crate::support::{build_test_app, patch_json_with_cookies, sign_up_with_authenticator};

const WRITE_LIMIT_PER_WINDOW: usize = 600;

#[sqlx::test]
async fn event_writes_share_the_600_writes_in_ten_minutes_limit(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let gym_event = create_planned_event(
        &db_pool,
        &signed_in_person,
        "Gym",
        "2026-11-05T09:00:00Z",
        "2026-11-05T10:00:00Z",
    )
    .await;
    let event_path = format!("/calendar/events/{}", event_id(&gym_event));
    let test_app = build_test_app(db_pool);

    for write_index in 1..WRITE_LIMIT_PER_WINDOW {
        let response = patch_json_with_cookies(
            test_app.clone(),
            &event_path,
            json!({ "title": format!("Gym {write_index}") }),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "write {} should be allowed",
            write_index + 1
        );
    }

    let blocked_response = patch_json_with_cookies(
        test_app,
        &event_path,
        json!({ "title": "One too many" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
}
