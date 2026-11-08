use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::cards::create::{NEW_CARD_NOTES, board_with_cards, card, card_id};
use crate::support::{
    build_test_app, get, get_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn reads_a_card_with_its_notes_and_linked_tasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        &format!("/cards/{}", card_id(&board, "Plan")),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "card": card(&board, "Plan"),
            "notes": NEW_CARD_NOTES,
            "linkedTasks": [],
            "trackedEventCount": 0,
            "schedules": [],
        })
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_card_or_a_malformed_id(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &owner, &[("Backlog", &["Plan"])]).await;

    for (card_path, cookie_header) in [
        (
            format!("/cards/{}", card_id(&board, "Plan")),
            &other_person.session_cookies,
        ),
        (String::from("/cards/not-a-uuid"), &owner.session_cookies),
    ] {
        let response =
            get_with_cookies(build_test_app(db_pool.clone()), &card_path, cookie_header).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test]
async fn reading_a_card_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(&db_pool, &signed_in_person, &[("Backlog", &["Plan"])]).await;

    let response = get(
        build_test_app(db_pool),
        &format!("/cards/{}", card_id(&board, "Plan")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
