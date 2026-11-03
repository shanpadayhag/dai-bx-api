use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id, list_places, places};
use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn renames_a_list_and_returns_the_board(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Backlog", "Doing"]).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}", list_id(&board, "Doing")),
        json!({ "name": "  In progress " }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        list_places(&response_json(response).await),
        places(&["Backlog", "In progress"])
    );
}

#[sqlx::test]
async fn refuses_a_new_list_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Backlog"]).await;

    for rejected_name in ["", &"a".repeat(101)] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/lists/{}", list_id(&board, "Backlog")),
            json!({ "name": rejected_name }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_list_or_a_malformed_id(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &owner, &["Backlog"]).await;

    for (list_path, cookie_header) in [
        (
            format!("/lists/{}", list_id(&board, "Backlog")),
            &other_person.session_cookies,
        ),
        (String::from("/lists/not-a-uuid"), &owner.session_cookies),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &list_path,
            json!({ "name": "Taken" }),
            cookie_header,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
