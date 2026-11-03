use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{build_test_app, patch_json_with_cookies, sign_up_with_authenticator};

const WRITE_LIMIT_PER_WINDOW: usize = 600;

#[sqlx::test]
async fn board_writes_share_the_600_writes_in_ten_minutes_limit(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let board_path = format!("/boards/{}", board_id(&created_board));
    let test_app = build_test_app(db_pool);

    for write_index in 1..WRITE_LIMIT_PER_WINDOW {
        let response = patch_json_with_cookies(
            test_app.clone(),
            &board_path,
            json!({ "name": format!("Personal {write_index}") }),
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
        &board_path,
        json!({ "name": "One too many" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(blocked_response.status(), StatusCode::TOO_MANY_REQUESTS);
}
