use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    build_test_app, get, get_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn a_new_account_has_no_boards(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/boards",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!([]));
}

#[sqlx::test]
async fn lists_only_the_person_s_boards_by_name_ignoring_case_then_oldest_first(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let studies_board = create_board(&db_pool, &signed_in_person, "studies").await;
    let first_personal_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let avvoka_board = create_board(&db_pool, &signed_in_person, "avvoka").await;
    let second_personal_board = create_board(&db_pool, &signed_in_person, "personal").await;
    create_board(&db_pool, &other_person, "Another person's board").await;

    let listed_boards = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/boards",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(
        listed_boards,
        json!([
            { "id": board_id(&avvoka_board), "name": "avvoka" },
            { "id": board_id(&first_personal_board), "name": "Personal" },
            { "id": board_id(&second_personal_board), "name": "personal" },
            { "id": board_id(&studies_board), "name": "studies" },
        ])
    );
}

#[sqlx::test]
async fn listing_boards_needs_a_session(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/boards").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
