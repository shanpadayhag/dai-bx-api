use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{add_list, list_id};
use crate::board_properties::create::{
    add_property, board_with_size_and_link, option_id, property_id,
};
use crate::boards::create::board_id;
use crate::cards::create::{NEW_CARD_NOTES, add_card, card_id};
use crate::support::{
    build_test_app, get_with_cookies, patch_json_with_cookies, put_json_with_cookies,
    response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn exports_a_list_s_cards_in_order_with_notes_and_named_values(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    add_property(
        &db_pool,
        &signed_in_person,
        board_id(&board),
        json!({ "name": "Hours", "kind": "number" }),
    )
    .await;
    add_list(&db_pool, &signed_in_person, board_id(&board), "Backlog").await;
    let board = add_list(&db_pool, &signed_in_person, board_id(&board), "Done").await;
    let backlog_list_id = list_id(&board, "Backlog").to_owned();
    add_card(
        &db_pool,
        &signed_in_person,
        list_id(&board, "Done"),
        "Shipped",
    )
    .await;
    add_card(
        &db_pool,
        &signed_in_person,
        &backlog_list_id,
        "Write the spec",
    )
    .await;
    let board = add_card(
        &db_pool,
        &signed_in_person,
        &backlog_list_id,
        "Plan the week",
    )
    .await;
    let spec_card_id = card_id(&board, "Write the spec").to_owned();
    patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/cards/{spec_card_id}"),
        json!({ "notes": "## Outcome\nA spec", "dueDate": "2026-11-05", "dueTime": "18:00" }),
        &signed_in_person.session_cookies,
    )
    .await;
    for (property_name, value_body) in [
        ("Hours", json!({ "number": "2.50" })),
        ("Link", json!({ "text": "https://example.com" })),
        (
            "Size",
            json!({ "optionId": option_id(&board, "Size", "Large") }),
        ),
    ] {
        let response = put_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!(
                "/cards/{spec_card_id}/values/{}",
                property_id(&board, property_name)
            ),
            value_body,
            &signed_in_person.session_cookies,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = get_with_cookies(
        build_test_app(db_pool),
        &format!("/lists/{backlog_list_id}/export"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "board": { "name": "Personal" },
            "list": { "name": "Backlog" },
            "cards": [
                {
                    "id": spec_card_id,
                    "title": "Write the spec",
                    "dueDate": "2026-11-05",
                    "dueTime": "18:00",
                    "values": [
                        { "property": "Size", "value": "Large" },
                        { "property": "Link", "value": "https://example.com" },
                        { "property": "Hours", "value": "2.50" },
                    ],
                    "notes": "## Outcome\nA spec",
                },
                {
                    "id": card_id(&board, "Plan the week"),
                    "title": "Plan the week",
                    "dueDate": null,
                    "dueTime": null,
                    "values": [],
                    "notes": NEW_CARD_NOTES,
                },
            ],
        })
    );
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_list_or_without_a_session(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &owner).await;
    let board = add_list(&db_pool, &owner, board_id(&board), "Backlog").await;
    let export_path = format!("/lists/{}/export", list_id(&board, "Backlog"));

    let foreign_export = get_with_cookies(
        build_test_app(db_pool.clone()),
        &export_path,
        &other_person.session_cookies,
    )
    .await;
    let signed_out_export = get_with_cookies(build_test_app(db_pool), &export_path, "").await;

    assert_eq!(foreign_export.status(), StatusCode::NOT_FOUND);
    assert_eq!(signed_out_export.status(), StatusCode::UNAUTHORIZED);
}
