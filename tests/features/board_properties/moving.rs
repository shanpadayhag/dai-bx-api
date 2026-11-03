use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_properties::create::{
    add_property, assert_error, board_with_size_and_link, option_id, option_names, property_id,
    property_names,
};
use crate::boards::create::board_id;
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn move_to(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    move_path: &str,
    position: i64,
) -> serde_json::Value {
    let response = post_json_with_cookies(
        build_test_app(db_pool.clone()),
        move_path,
        json!({ "position": position }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn moves_a_property_keeping_every_position_contiguous(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    let board = add_property(
        &db_pool,
        &signed_in_person,
        board_id(&board),
        json!({ "name": "Hours", "kind": "number" }),
    )
    .await;
    let hours_move_path = format!("/properties/{}/move", property_id(&board, "Hours"));

    let moved_first = move_to(&db_pool, &signed_in_person, &hours_move_path, 0).await;
    let moved_past_the_end = move_to(&db_pool, &signed_in_person, &hours_move_path, 99).await;

    assert_eq!(property_names(&moved_first), vec!["Hours", "Size", "Link"]);
    assert_eq!(
        property_names(&moved_past_the_end),
        vec!["Size", "Link", "Hours"]
    );
}

#[sqlx::test]
async fn moves_an_option_keeping_every_position_contiguous(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    let small_move_path = format!("/options/{}/move", option_id(&board, "Size", "Small"));

    let moved_middle = move_to(&db_pool, &signed_in_person, &small_move_path, 1).await;
    let moved_past_the_end = move_to(&db_pool, &signed_in_person, &small_move_path, 99).await;

    assert_eq!(
        option_names(&moved_middle, "Size"),
        vec!["Medium", "Small", "Large"]
    );
    assert_eq!(
        option_names(&moved_past_the_end, "Size"),
        vec!["Medium", "Large", "Small"]
    );
}

#[sqlx::test]
async fn refuses_a_negative_position(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    for move_path in [
        format!("/properties/{}/move", property_id(&board, "Link")),
        format!("/options/{}/move", option_id(&board, "Size", "Large")),
    ] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &move_path,
            json!({ "position": -1 }),
            &signed_in_person.session_cookies,
        )
        .await;
        assert_error(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "a position is 0 or more",
        )
        .await;
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_property_or_option(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &owner).await;

    for move_path in [
        format!("/properties/{}/move", property_id(&board, "Link")),
        format!("/options/{}/move", option_id(&board, "Size", "Large")),
    ] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            &move_path,
            json!({ "position": 0 }),
            &other_person.session_cookies,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
