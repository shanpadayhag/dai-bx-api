use axum::body::Body;
use axum::http::{Response, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use crate::boards::create::{board_id, create_board};
use crate::support::{
    SignedInPerson, build_test_app, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn post_property(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_id: &str,
    property_body: serde_json::Value,
) -> Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/boards/{board_id}/properties"),
        property_body,
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn add_property(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board_id: &str,
    property_body: serde_json::Value,
) -> serde_json::Value {
    let response = post_property(db_pool, signed_in_person, board_id, property_body).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub async fn board_with_size_and_link(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let created_board = create_board(db_pool, signed_in_person, "Personal").await;
    add_property(
        db_pool,
        signed_in_person,
        board_id(&created_board),
        json!({
            "name": "Size",
            "kind": "choice",
            "options": [
                { "name": "Small", "color": "#22c55e" },
                { "name": "Medium" },
                { "name": "Large", "color": "#ef4444" },
            ],
        }),
    )
    .await;
    add_property(
        db_pool,
        signed_in_person,
        board_id(&created_board),
        json!({ "name": "Link", "kind": "text" }),
    )
    .await
}

pub fn property<'a>(board: &'a serde_json::Value, property_name: &str) -> &'a serde_json::Value {
    board["properties"]
        .as_array()
        .expect("a property array")
        .iter()
        .find(|board_property| board_property["name"] == property_name)
        .expect("the property should be on the board")
}

pub fn property_id<'a>(board: &'a serde_json::Value, property_name: &str) -> &'a str {
    property(board, property_name)["id"]
        .as_str()
        .expect("a property id")
}

pub fn option_id<'a>(
    board: &'a serde_json::Value,
    property_name: &str,
    option_name: &str,
) -> &'a str {
    property(board, property_name)["options"]
        .as_array()
        .expect("an option array")
        .iter()
        .find(|property_option| property_option["name"] == option_name)
        .expect("the option should be on the property")["id"]
        .as_str()
        .expect("an option id")
}

fn names_in_position_order(named_rows: &[serde_json::Value], label: &str) -> Vec<String> {
    let positions: Vec<i64> = named_rows
        .iter()
        .map(|named_row| named_row["position"].as_i64().expect("a position"))
        .collect();
    assert_eq!(
        positions,
        (0..named_rows.len() as i64).collect::<Vec<_>>(),
        "{label} positions should run 0, 1, 2… in order"
    );
    named_rows
        .iter()
        .map(|named_row| named_row["name"].as_str().expect("a name").to_owned())
        .collect()
}

pub fn property_names(board: &serde_json::Value) -> Vec<String> {
    names_in_position_order(
        board["properties"].as_array().expect("a property array"),
        "property",
    )
}

pub fn option_names(board: &serde_json::Value, property_name: &str) -> Vec<String> {
    names_in_position_order(
        property(board, property_name)["options"]
            .as_array()
            .expect("an option array"),
        "option",
    )
}

pub async fn assert_error(
    response: Response<Body>,
    expected_status: StatusCode,
    expected_message: &str,
) {
    assert_eq!(response.status(), expected_status);
    assert_eq!(
        response_json(response).await,
        json!({ "error": expected_message })
    );
}

#[sqlx::test]
async fn adds_each_property_last_with_trimmed_names_and_lowercase_colors(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    add_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({
            "name": " Size ",
            "kind": "choice",
            "options": [{ "name": " Small ", "color": "#22C55E" }, { "name": "Large" }],
        }),
    )
    .await;
    add_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "Hours", "kind": "number" }),
    )
    .await;
    let board = add_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "Link", "kind": "text", "options": [] }),
    )
    .await;

    assert_eq!(property_names(&board), vec!["Size", "Hours", "Link"]);
    assert_eq!(
        property(&board, "Size"),
        &json!({
            "id": property_id(&board, "Size"),
            "name": "Size",
            "kind": "choice",
            "position": 0,
            "options": [
                {
                    "id": option_id(&board, "Size", "Small"),
                    "name": "Small",
                    "color": "#22c55e",
                    "position": 0,
                },
                {
                    "id": option_id(&board, "Size", "Large"),
                    "name": "Large",
                    "color": null,
                    "position": 1,
                },
            ],
        })
    );
    assert_eq!(property(&board, "Hours")["kind"], "number");
    assert_eq!(property(&board, "Link")["options"], json!([]));
}

#[sqlx::test]
async fn answers_conflict_for_a_name_already_taken_in_any_case(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let taken_property = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&board),
        json!({ "name": "LINK", "kind": "number" }),
    )
    .await;
    let taken_option = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&board),
        json!({
            "name": "Project",
            "kind": "choice",
            "options": [{ "name": "Chinese" }, { "name": "chinese" }],
        }),
    )
    .await;

    assert_error(
        taken_property,
        StatusCode::CONFLICT,
        "this board already has a property with that name",
    )
    .await;
    assert_error(
        taken_option,
        StatusCode::CONFLICT,
        "this property already has an option with that name",
    )
    .await;
}

#[sqlx::test]
async fn refuses_a_choice_without_options_and_options_on_other_kinds(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;

    let empty_choice = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "Size", "kind": "choice" }),
    )
    .await;
    let text_with_options = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "Link", "kind": "text", "options": [{ "name": "A" }] }),
    )
    .await;

    assert_error(
        empty_choice,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a choice property needs at least one option",
    )
    .await;
    assert_error(
        text_with_options,
        StatusCode::UNPROCESSABLE_ENTITY,
        "only a choice property has options",
    )
    .await;
}

#[sqlx::test]
async fn refuses_a_bad_name_or_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let long_name = "a".repeat(51);

    for (property_body, expected_message) in [
        (
            json!({ "name": "  ", "kind": "text" }),
            "a property name is 1 to 50 characters",
        ),
        (
            json!({ "name": long_name, "kind": "text" }),
            "a property name is 1 to 50 characters",
        ),
        (
            json!({ "name": "Size", "kind": "choice", "options": [{ "name": long_name }] }),
            "an option name is 1 to 50 characters",
        ),
        (
            json!({ "name": "Size", "kind": "choice", "options": [{ "name": "S", "color": "green" }] }),
            "a color is # followed by six hex digits, like #8fae9a",
        ),
    ] {
        let response = post_property(
            &db_pool,
            &signed_in_person,
            board_id(&created_board),
            property_body,
        )
        .await;
        assert_error(response, StatusCode::UNPROCESSABLE_ENTITY, expected_message).await;
    }
}

#[sqlx::test]
async fn refuses_a_twenty_first_property_and_a_fifty_first_option(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &signed_in_person, "Personal").await;
    let mut board = created_board.clone();
    for property_number in 1..=20 {
        board = add_property(
            &db_pool,
            &signed_in_person,
            board_id(&created_board),
            json!({ "name": format!("Property {property_number}"), "kind": "text" }),
        )
        .await;
    }
    let fifty_one_options: Vec<serde_json::Value> = (1..=51)
        .map(|option_number| json!({ "name": format!("Option {option_number}") }))
        .collect();

    let twenty_first_property = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&created_board),
        json!({ "name": "One more", "kind": "number" }),
    )
    .await;
    let other_board = create_board(&db_pool, &signed_in_person, "Studies").await;
    let fifty_first_option = post_property(
        &db_pool,
        &signed_in_person,
        board_id(&other_board),
        json!({ "name": "Size", "kind": "choice", "options": fifty_one_options }),
    )
    .await;

    assert_eq!(property_names(&board).len(), 20);
    assert_error(
        twenty_first_property,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a board has at most 20 properties",
    )
    .await;
    assert_error(
        fifty_first_option,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a choice property has at most 50 options",
    )
    .await;
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_board_or_a_malformed_id(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let created_board = create_board(&db_pool, &owner, "Personal").await;

    let foreign_create = post_property(
        &db_pool,
        &other_person,
        board_id(&created_board),
        json!({ "name": "Link", "kind": "text" }),
    )
    .await;
    let malformed_create = post_property(
        &db_pool,
        &owner,
        "not-a-uuid",
        json!({ "name": "Link", "kind": "text" }),
    )
    .await;

    assert_eq!(foreign_create.status(), StatusCode::NOT_FOUND);
    assert_eq!(malformed_create.status(), StatusCode::NOT_FOUND);
}
