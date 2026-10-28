use axum::http::{StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::categories::update::find_category_id;
use crate::support::{build_test_app, patch_json_with_cookies, sign_up_with_authenticator};

const WRITE_LIMIT_PER_WINDOW: usize = 600;

#[sqlx::test]
async fn allows_600_writes_in_ten_minutes_then_answers_429_with_retry_after(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;
    let work_category_path = format!("/categories/{work_category_id}");
    let test_app = build_test_app(db_pool);

    for write_index in 0..WRITE_LIMIT_PER_WINDOW {
        let response = patch_json_with_cookies(
            test_app.clone(),
            &work_category_path,
            json!({ "color": format!("#{:06x}", write_index) }),
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
        &work_category_path,
        json!({ "color": "#ffffff" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after_seconds: u64 = blocked_response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|retry_after| retry_after.to_str().ok())
        .and_then(|retry_after| retry_after.parse().ok())
        .expect("a Retry-After header in seconds");
    assert!(
        (540..=600).contains(&retry_after_seconds),
        "expected the rest of a ten-minute window, got {retry_after_seconds}"
    );
}
