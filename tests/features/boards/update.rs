use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn renames_a_board_and_returns_it_whole(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{}", board_id(&created_board)),
        json!({ "name": "  Home  " }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "board": { "id": board_id(&created_board), "name": "Home" },
            "lists": [],
            "cards": [],
            "properties": [],
        })
    );
}

#[sqlx::test]
async fn refuses_a_new_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    for rejected_name in ["", &"a".repeat(101)] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/boards/{}", board_id(&created_board)),
            json!({ "name": rejected_name }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a board name is 1 to 100 characters" })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_board_or_a_malformed_id(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_board = create_board(&db_pool, &owner, "Mine").await;

    for (board_path, cookie_header) in [
        (
            format!("/boards/{}", board_id(&owner_board)),
            &other_person.session_cookies,
        ),
        (String::from("/boards/not-a-uuid"), &owner.session_cookies),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &board_path,
            json!({ "name": "Taken" }),
            cookie_header,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn renaming_a_board_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{}", board_id(&created_board)),
        json!({ "name": "Home" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
