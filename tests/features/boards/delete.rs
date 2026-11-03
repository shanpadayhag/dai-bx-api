use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_board_and_everything_inside_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let personal_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let studies_board = create_board(&db_pool, &signed_in_person, "Studies").await;
    let personal_board_path = format!("/boards/{}", board_id(&personal_board));

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &personal_board_path,
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "status": "deleted" })
    );
    let deleted_board_read = get_with_cookies(
        build_test_app(db_pool.clone()),
        &personal_board_path,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(deleted_board_read.status(), StatusCode::NOT_FOUND);
    let remaining_boards = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/boards",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    assert_eq!(
        remaining_boards,
        json!([{ "id": board_id(&studies_board), "name": "Studies" }])
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_a_deleted_or_a_malformed_board(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_board = create_board(&db_pool, &owner, "Mine").await;
    let owner_board_path = format!("/boards/{}", board_id(&owner_board));

    let foreign_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_board_path,
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_board_path,
        &owner.session_cookies,
    )
    .await;
    let repeated_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_board_path,
        &owner.session_cookies,
    )
    .await;
    let malformed_delete = delete_with_cookies(
        build_test_app(db_pool),
        "/boards/not-a-uuid",
        &owner.session_cookies,
    )
    .await;

    assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(malformed_delete.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_board_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let personal_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/boards/{}", board_id(&personal_board)),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
