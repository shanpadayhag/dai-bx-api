use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{build_test_app, patch_json_with_cookies, sign_up_with_authenticator};

const WRITE_LIMIT_PER_WINDOW: usize = 600;

#[sqlx::test]
async fn account_changes_share_the_600_writes_in_ten_minutes_limit(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let test_app = build_test_app(db_pool);

    for write_number in 1..=WRITE_LIMIT_PER_WINDOW {
        let response = patch_json_with_cookies(
            test_app.clone(),
            "/account",
            json!({ "theme": "dark" }),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "write {write_number} should be allowed"
        );
    }

    let blocked_profile_response = patch_json_with_cookies(
        test_app.clone(),
        "/account",
        json!({ "theme": "light" }),
        &signed_in_person.session_cookies,
    )
    .await;
    let blocked_tracking_response = patch_json_with_cookies(
        test_app,
        "/account/time-tracking",
        json!({ "checkInsEnabled": true }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(
        blocked_profile_response.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        blocked_tracking_response.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
