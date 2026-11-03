use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::add_list;
use crate::board_properties::create::{
    assert_error, board_with_size_and_link, option_id, option_names, property_id, property_names,
};
use crate::boards::create::board_id;
use crate::cards::create::{add_card, card, card_id};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, response_json, sign_up_with_authenticator,
};

async fn board_with_valued_cards(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let board = board_with_size_and_link(db_pool, signed_in_person).await;
    let board = add_list(db_pool, signed_in_person, board_id(&board), "Backlog").await;
    let backlog_list_id = board["lists"][0]["id"]
        .as_str()
        .expect("a list id")
        .to_owned();
    add_card(db_pool, signed_in_person, &backlog_list_id, "Small card").await;
    let board = add_card(db_pool, signed_in_person, &backlog_list_id, "Large card").await;
    for (card_title, option_name, link_text) in [
        ("Small card", "Small", Some("https://example.com")),
        ("Large card", "Large", None),
    ] {
        store_value(
            db_pool,
            card_id(&board, card_title),
            property_id(&board, "Size"),
            Some(option_id(&board, "Size", option_name)),
            None,
        )
        .await;
        if let Some(link_text) = link_text {
            store_value(
                db_pool,
                card_id(&board, card_title),
                property_id(&board, "Link"),
                None,
                Some(link_text),
            )
            .await;
        }
    }
    board
}

async fn store_value(
    db_pool: &PgPool,
    card_id: &str,
    property_id: &str,
    option_id: Option<&str>,
    text_value: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, option_id, text_value)
         SELECT user_id, board_id, id, $2::text::uuid, $3::text::uuid, $4
         FROM cards
         WHERE id = $1::text::uuid",
    )
    .bind(card_id)
    .bind(property_id)
    .bind(option_id)
    .bind(text_value)
    .execute(db_pool)
    .await
    .expect("the value should be stored");
}

async fn delete_and_read(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    delete_path: &str,
) -> serde_json::Value {
    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        delete_path,
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

#[sqlx::test]
async fn deleting_a_property_clears_it_from_every_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_valued_cards(&db_pool, &signed_in_person).await;

    let updated_board = delete_and_read(
        &db_pool,
        &signed_in_person,
        &format!("/properties/{}", property_id(&board, "Size")),
    )
    .await;

    assert_eq!(property_names(&updated_board), vec!["Link"]);
    assert_eq!(
        card(&updated_board, "Small card")["values"],
        json!([{ "propertyId": property_id(&board, "Link"), "text": "https://example.com" }])
    );
    assert_eq!(card(&updated_board, "Large card")["values"], json!([]));
}

#[sqlx::test]
async fn deleting_an_option_clears_only_the_cards_using_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_valued_cards(&db_pool, &signed_in_person).await;

    let updated_board = delete_and_read(
        &db_pool,
        &signed_in_person,
        &format!("/options/{}", option_id(&board, "Size", "Small")),
    )
    .await;

    assert_eq!(
        option_names(&updated_board, "Size"),
        vec!["Medium", "Large"]
    );
    assert_eq!(
        card(&updated_board, "Small card")["values"],
        json!([{ "propertyId": property_id(&board, "Link"), "text": "https://example.com" }])
    );
    assert_eq!(
        card(&updated_board, "Large card")["values"],
        json!([{
            "propertyId": property_id(&board, "Size"),
            "optionId": option_id(&board, "Size", "Large"),
        }])
    );
}

#[sqlx::test]
async fn refuses_to_delete_a_choice_s_last_option(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    delete_and_read(
        &db_pool,
        &signed_in_person,
        &format!("/options/{}", option_id(&board, "Size", "Small")),
    )
    .await;
    delete_and_read(
        &db_pool,
        &signed_in_person,
        &format!("/options/{}", option_id(&board, "Size", "Medium")),
    )
    .await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/options/{}", option_id(&board, "Size", "Large")),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a choice property keeps at least one option",
    )
    .await;
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_an_already_deleted_property(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &owner).await;
    let link_path = format!("/properties/{}", property_id(&board, "Link"));
    let small_path = format!("/options/{}", option_id(&board, "Size", "Small"));

    let foreign_property_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &link_path,
        &other_person.session_cookies,
    )
    .await;
    let foreign_option_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &small_path,
        &other_person.session_cookies,
    )
    .await;
    delete_and_read(&db_pool, &owner, &link_path).await;
    let repeated_delete =
        delete_with_cookies(build_test_app(db_pool), &link_path, &owner.session_cookies).await;

    assert_eq!(foreign_property_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_option_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
}
