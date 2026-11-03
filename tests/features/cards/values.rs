use axum::body::Body;
use axum::http::{Response, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::add_list;
use crate::board_properties::create::{
    add_property, assert_error, board_with_size_and_link, option_id, property_id,
};
use crate::boards::create::{board_id, create_board};
use crate::cards::create::{add_card, card, card_id};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, put_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

async fn board_with_a_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let board = board_with_size_and_link(db_pool, signed_in_person).await;
    add_property(
        db_pool,
        signed_in_person,
        board_id(&board),
        json!({ "name": "Effort", "kind": "choice", "options": [{ "name": "Low" }] }),
    )
    .await;
    let board = add_property(
        db_pool,
        signed_in_person,
        board_id(&board),
        json!({ "name": "Hours", "kind": "number" }),
    )
    .await;
    let board = add_list(db_pool, signed_in_person, board_id(&board), "Backlog").await;
    let backlog_list_id = board["lists"][0]["id"]
        .as_str()
        .expect("a list id")
        .to_owned();
    add_card(db_pool, signed_in_person, &backlog_list_id, "Plan").await
}

async fn put_value(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
    property_id: &str,
    value_body: serde_json::Value,
) -> Response<Body> {
    put_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{card_id}/values/{property_id}"),
        value_body,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn set_value(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board: &serde_json::Value,
    property_name: &str,
    value_body: serde_json::Value,
) -> serde_json::Value {
    let response = put_value(
        db_pool,
        signed_in_person,
        card_id(board, "Plan"),
        property_id(board, property_name),
        value_body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn clear_value(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    board: &serde_json::Value,
    property_name: &str,
) -> Response<Body> {
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!(
            "/cards/{}/values/{}",
            card_id(board, "Plan"),
            property_id(board, property_name)
        ),
        &signed_in_person.session_cookies,
    )
    .await
}

#[sqlx::test]
async fn sets_each_kind_in_property_order_keeping_numbers_exact(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &signed_in_person).await;

    set_value(
        &db_pool,
        &signed_in_person,
        &board,
        "Hours",
        json!({ "number": "2.50" }),
    )
    .await;
    set_value(
        &db_pool,
        &signed_in_person,
        &board,
        "Link",
        json!({ "text": "  https://example.com " }),
    )
    .await;
    set_value(
        &db_pool,
        &signed_in_person,
        &board,
        "Size",
        json!({ "optionId": option_id(&board, "Size", "Small") }),
    )
    .await;
    let updated_board = set_value(
        &db_pool,
        &signed_in_person,
        &board,
        "Size",
        json!({ "optionId": option_id(&board, "Size", "Large") }),
    )
    .await;

    assert_eq!(
        card(&updated_board, "Plan")["values"],
        json!([
            { "propertyId": property_id(&board, "Size"), "optionId": option_id(&board, "Size", "Large") },
            { "propertyId": property_id(&board, "Link"), "text": "https://example.com" },
            { "propertyId": property_id(&board, "Hours"), "number": "2.50" },
        ])
    );
}

#[sqlx::test]
async fn clears_each_kind_and_accepts_clearing_an_unset_value(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &signed_in_person).await;
    for (property_name, value_body) in [
        (
            "Size",
            json!({ "optionId": option_id(&board, "Size", "Small") }),
        ),
        ("Link", json!({ "text": "notes" })),
        ("Hours", json!({ "number": "-1" })),
    ] {
        set_value(
            &db_pool,
            &signed_in_person,
            &board,
            property_name,
            value_body,
        )
        .await;
    }

    let mut last_response = None;
    for property_name in ["Size", "Link", "Hours", "Effort"] {
        let response = clear_value(&db_pool, &signed_in_person, &board, property_name).await;
        assert_eq!(response.status(), StatusCode::OK, "{property_name}");
        last_response = Some(response);
    }

    let cleared_board = response_json(last_response.expect("a response")).await;
    assert_eq!(card(&cleared_board, "Plan")["values"], json!([]));
}

#[sqlx::test]
async fn refuses_a_value_of_the_wrong_kind(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &signed_in_person).await;

    for (property_name, value_body, expected_message) in [
        (
            "Size",
            json!({ "text": "Small" }),
            "this property takes an optionId",
        ),
        ("Link", json!({ "number": "3" }), "this property takes text"),
        (
            "Hours",
            json!({ "optionId": option_id(&board, "Size", "Small") }),
            "this property takes a number",
        ),
    ] {
        let response = put_value(
            &db_pool,
            &signed_in_person,
            card_id(&board, "Plan"),
            property_id(&board, property_name),
            value_body,
        )
        .await;
        assert_error(response, StatusCode::UNPROCESSABLE_ENTITY, expected_message).await;
    }
}

#[sqlx::test]
async fn refuses_another_property_s_option_or_a_malformed_one(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &signed_in_person).await;

    for value_body in [
        json!({ "optionId": option_id(&board, "Effort", "Low") }),
        json!({ "optionId": "not-a-uuid" }),
    ] {
        let response = put_value(
            &db_pool,
            &signed_in_person,
            card_id(&board, "Plan"),
            property_id(&board, "Size"),
            value_body,
        )
        .await;
        assert_error(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "that option isn't one of this property's options",
        )
        .await;
    }
}

#[sqlx::test]
async fn refuses_bad_text_numbers_or_value_shapes(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &signed_in_person).await;
    let number_message = "a number has at most 15 digits before the point and 6 after, like 2.50";

    for (property_name, value_body, expected_message) in [
        (
            "Link",
            json!({ "text": "a".repeat(501) }),
            "a text value is 1 to 500 characters",
        ),
        (
            "Link",
            json!({ "text": "   " }),
            "a text value is 1 to 500 characters",
        ),
        (
            "Hours",
            json!({ "number": "1000000000000000" }),
            number_message,
        ),
        ("Hours", json!({ "number": "1.0000001" }), number_message),
        ("Hours", json!({ "number": "1e3" }), number_message),
        (
            "Hours",
            json!({}),
            "send exactly one of optionId, text or number",
        ),
        (
            "Hours",
            json!({ "text": "1", "number": "1" }),
            "send exactly one of optionId, text or number",
        ),
    ] {
        let response = put_value(
            &db_pool,
            &signed_in_person,
            card_id(&board, "Plan"),
            property_id(&board, property_name),
            value_body,
        )
        .await;
        assert_error(response, StatusCode::UNPROCESSABLE_ENTITY, expected_message).await;
    }
}

#[sqlx::test]
async fn answers_not_found_for_another_board_s_property_or_someone_else_s_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_a_card(&db_pool, &owner).await;
    let other_board = create_board(&db_pool, &owner, "Studies").await;
    let other_board = add_property(
        &db_pool,
        &owner,
        board_id(&other_board),
        json!({ "name": "Link", "kind": "text" }),
    )
    .await;

    let other_board_property = put_value(
        &db_pool,
        &owner,
        card_id(&board, "Plan"),
        property_id(&other_board, "Link"),
        json!({ "text": "x" }),
    )
    .await;
    let foreign_set = put_value(
        &db_pool,
        &other_person,
        card_id(&board, "Plan"),
        property_id(&board, "Link"),
        json!({ "text": "x" }),
    )
    .await;
    let foreign_clear = clear_value(&db_pool, &other_person, &board, "Link").await;
    let other_board_clear = delete_with_cookies(
        build_test_app(db_pool),
        &format!(
            "/cards/{}/values/{}",
            card_id(&board, "Plan"),
            property_id(&other_board, "Link")
        ),
        &owner.session_cookies,
    )
    .await;

    assert_eq!(other_board_property.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_set.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_clear.status(), StatusCode::NOT_FOUND);
    assert_eq!(other_board_clear.status(), StatusCode::NOT_FOUND);
}
