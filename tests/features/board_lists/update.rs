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

async fn patch_archive_after(
    db_pool: &PgPool,
    signed_in_person: &crate::support::SignedInPerson,
    list_id: &str,
    archive_after: serde_json::Value,
) -> axum::response::Response {
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{list_id}"),
        json!({ "archiveAfter": archive_after }),
        &signed_in_person.session_cookies,
    )
    .await
}

fn archive_after_of(board: &serde_json::Value, list_name: &str) -> serde_json::Value {
    board["lists"]
        .as_array()
        .and_then(|lists| lists.iter().find(|list| list["name"] == list_name))
        .expect("the list should be on the board")["archiveAfter"]
        .clone()
}

#[sqlx::test]
async fn sets_changes_and_clears_a_list_s_archive_after(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Backlog", "Done"]).await;
    let done_list_id = list_id(&board, "Done").to_owned();
    assert_eq!(archive_after_of(&board, "Done"), json!(null));

    let mut archive_after_steps = Vec::new();
    for archive_after in [json!("10d"), json!("1M 2W"), json!(null)] {
        let response =
            patch_archive_after(&db_pool, &signed_in_person, &done_list_id, archive_after).await;
        assert_eq!(response.status(), StatusCode::OK);
        let patched_board = response_json(response).await;
        assert_eq!(archive_after_of(&patched_board, "Backlog"), json!(null));
        archive_after_steps.push(archive_after_of(&patched_board, "Done"));
    }

    assert_eq!(
        archive_after_steps,
        vec![json!("1w 3d"), json!("1m 2w"), json!(null)]
    );
}

#[sqlx::test]
async fn refuses_a_bad_archive_after_with_the_hint(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &signed_in_person, &["Done"]).await;

    for rejected_length in ["", "0d", "3x", "2d 1d", "13m", "soon"] {
        let response = patch_archive_after(
            &db_pool,
            &signed_in_person,
            list_id(&board, "Done"),
            json!(rejected_length),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "Use days, weeks or months, like 3d, 2w or 1m" })
        );
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_archive_after(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_lists(&db_pool, &owner, &["Done"]).await;

    let response = patch_archive_after(
        &db_pool,
        &other_person,
        list_id(&board, "Done"),
        json!("1w"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
