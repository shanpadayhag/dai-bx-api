use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, post_json, post_json_with_cookies, response_json, send,
    sign_up_with_authenticator,
};

pub async fn create_board(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_name: &str,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/boards",
        json!({ "name": board_name }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

pub fn board_id(board: &serde_json::Value) -> &str {
    board["board"]["id"].as_str().expect("a board id")
}

#[sqlx::test]
async fn creates_an_empty_board_with_a_trimmed_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let created_board = create_board(&db_pool, &signed_in_person, "  Personal  ").await;

    assert_eq!(
        created_board,
        json!({
            "board": { "id": board_id(&created_board), "name": "Personal" },
            "lists": [],
            "cards": [],
            "properties": [],
        })
    );
}

#[sqlx::test]
async fn refuses_a_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for rejected_name in ["   ", &"a".repeat(101)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/boards",
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
    create_board(
        &db_pool,
        &signed_in_person,
        &format!(" {} ", "a".repeat(100)),
    )
    .await;
}

#[sqlx::test]
async fn creating_a_board_needs_a_session(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool),
        "/boards",
        json!({ "name": "Personal" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn refuses_a_cross_site_board_write(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let cross_site_request = Request::post("/boards")
        .header("sec-fetch-site", "cross-site")
        .header("content-type", "application/json")
        .header(header::COOKIE, &signed_in_person.session_cookies)
        .body(Body::from(json!({ "name": "Personal" }).to_string()))
        .expect("test request should build");

    let response = send(build_test_app(db_pool.clone()), cross_site_request).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let listed_boards = response_json(
        crate::support::get_with_cookies(
            build_test_app(db_pool),
            "/boards",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    assert_eq!(listed_boards, json!([]));
}
