use axum::http::StatusCode;
use sqlx::PgPool;

use crate::board_lists::create::{board_with_lists, list_id, list_places, places};
use crate::boards::create::board_id;
use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_list_and_closes_the_gap_it_leaves(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["A", "B", "C"]).await;

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{}", list_id(&board, "B")),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        list_places(&response_json(response).await),
        places(&["A", "C"])
    );
    let reread_board = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            &format!("/boards/{}", board_id(&board)),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    assert_eq!(list_places(&reread_board), places(&["A", "C"]));
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_an_already_deleted_list(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &owner, &["A"]).await;
    let list_path = format!("/lists/{}", list_id(&board, "A"));

    let foreign_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &list_path,
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &list_path,
        &owner.session_cookies,
    )
    .await;
    let repeated_delete =
        delete_with_cookies(build_test_app(db_pool), &list_path, &owner.session_cookies).await;

    assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_list_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["A"]).await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{}", list_id(&board, "A")),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn deleting_a_list_removes_its_cards(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = crate::cards::create::board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Backlog", &["A", "B"]), ("Done", &["C"])],
    )
    .await;

    let after_delete = response_json(
        delete_with_cookies(
            build_test_app(db_pool),
            &format!("/lists/{}", list_id(&board, "Backlog")),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(
        crate::cards::create::card_titles_by_list(&after_delete, "Done"),
        vec!["C"]
    );
    assert_eq!(after_delete["cards"].as_array().expect("cards").len(), 1);
}
