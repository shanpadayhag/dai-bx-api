use axum::body::Body;
use axum::http::{Response, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use crate::board_properties::create::{
    add_property, assert_error, board_with_size_and_link, option_names, property, property_id,
};
use crate::boards::create::{board_id, create_board};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn post_option(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    property_id: &str,
    option_body: serde_json::Value,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/properties/{property_id}/options"),
        option_body,
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn adds_an_option_last_with_a_lowercase_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let response = post_option(
        &db_pool,
        &signed_in_person,
        property_id(&board, "Size"),
        json!({ "name": " Huge ", "color": "#A855F7" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_board = response_json(response).await;
    assert_eq!(
        option_names(&updated_board, "Size"),
        vec!["Small", "Medium", "Large", "Huge"]
    );
    assert_eq!(
        property(&updated_board, "Size")["options"][3]["color"],
        "#a855f7"
    );
}

#[sqlx::test]
async fn answers_conflict_for_an_option_name_taken_in_any_case(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let response = post_option(
        &db_pool,
        &signed_in_person,
        property_id(&board, "Size"),
        json!({ "name": "MEDIUM" }),
    )
    .await;

    assert_error(
        response,
        StatusCode::CONFLICT,
        "this property already has an option with that name",
    )
    .await;
}

#[sqlx::test]
async fn refuses_an_option_on_a_text_property_or_with_a_bad_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let text_option = post_option(
        &db_pool,
        &signed_in_person,
        property_id(&board, "Link"),
        json!({ "name": "A" }),
    )
    .await;
    let bad_color = post_option(
        &db_pool,
        &signed_in_person,
        property_id(&board, "Size"),
        json!({ "name": "Huge", "color": "#12345" }),
    )
    .await;

    assert_error(
        text_option,
        StatusCode::UNPROCESSABLE_ENTITY,
        "only a choice property has options",
    )
    .await;
    assert_error(
        bad_color,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a color is # followed by six hex digits, like #8fae9a",
    )
    .await;
}

#[sqlx::test]
async fn refuses_a_fifty_first_option(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let fifty_options: Vec<serde_json::Value> = (1..=50)
        .map(|option_number| json!({ "name": format!("Option {option_number}") }))
        .collect();
    let board = add_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "Project", "kind": "choice", "options": fifty_options }),
    )
    .await;

    let response = post_option(
        &db_pool,
        &signed_in_person,
        property_id(&board, "Project"),
        json!({ "name": "One more" }),
    )
    .await;

    assert_eq!(option_names(&board, "Project").len(), 50);
    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a choice property has at most 50 options",
    )
    .await;
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_property(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &owner).await;

    let response = post_option(
        &db_pool,
        &other_person,
        property_id(&board, "Size"),
        json!({ "name": "Huge" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
