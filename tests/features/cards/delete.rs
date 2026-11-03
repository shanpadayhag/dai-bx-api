use axum::http::StatusCode;
use sqlx::PgPool;

use crate::cards::create::{board_with_cards, card_id, card_titles_by_list};
use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_card_and_closes_the_gap_it_leaves(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Backlog", &["A", "B", "C"])],
    )
    .await;
    let card_path = format!("/cards/{}", card_id(&board, "B"));

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &card_path,
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        card_titles_by_list(&response_json(response).await, "Backlog"),
        vec!["A", "C"]
    );
    let deleted_card_read = get_with_cookies(
        build_test_app(db_pool),
        &card_path,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(deleted_card_read.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_an_already_deleted_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("Backlog", &["A"])]).await;
    let card_path = format!("/cards/{}", card_id(&board, "A"));

    let foreign_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &card_path,
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &card_path,
        &owner.session_cookies,
    )
    .await;
    let repeated_delete =
        delete_with_cookies(build_test_app(db_pool), &card_path, &owner.session_cookies).await;

    assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
}
