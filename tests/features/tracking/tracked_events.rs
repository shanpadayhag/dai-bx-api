use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::calendar_events::create::event_id;
use crate::categories::update::find_category_id;
use crate::support::{
    build_test_app, delete_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tracking::start::{add_top_level_task, start_task};

#[sqlx::test]
async fn a_running_event_changes_its_category_but_not_its_name_or_times(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let event_path = format!("/calendar/events/{}", event_id(&running_item));

    let category_response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &event_path,
        json!({ "categoryId": studies_category_id }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(category_response.status(), StatusCode::OK);
    let recategorized_event = response_json(category_response).await;
    assert_eq!(
        recategorized_event["categoryId"],
        studies_category_id.as_str()
    );
    assert_eq!(recategorized_event["endsAt"], serde_json::Value::Null);
    for refused_change in [
        json!({ "title": "Something else" }),
        json!({ "start": "2025-11-05T09:00", "inputZone": "UTC" }),
        json!({ "end": "2025-11-05T10:00", "inputZone": "UTC" }),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &event_path,
            refused_change.clone(),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_change}"
        );
    }
}

#[sqlx::test]
async fn a_running_event_can_not_be_deleted(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/calendar/events/{}", event_id(&running_item)),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
