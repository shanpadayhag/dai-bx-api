use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    build_test_app, get, get_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn reads_the_person_s_board(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{}", board_id(&created_board)),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, created_board);
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
        let response =
            get_with_cookies(build_test_app(db_pool.clone()), &board_path, cookie_header).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "not found" })
        );
    }
}

#[sqlx::test]
async fn reading_a_board_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = get(
        build_test_app(db_pool),
        &format!("/boards/{}", board_id(&created_board)),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
