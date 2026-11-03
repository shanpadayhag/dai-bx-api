use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_properties::create::{
    assert_error, board_with_size_and_link, option_id, option_names, property, property_id,
    property_names,
};
use crate::support::{
    build_test_app, patch_json_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn renames_a_property_keeping_its_kind_and_options(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/properties/{}", property_id(&board, "Size")),
        json!({ "name": " Effort " }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let renamed_board = response_json(response).await;
    assert_eq!(property_names(&renamed_board), vec!["Effort", "Link"]);
    assert_eq!(property(&renamed_board, "Effort")["kind"], "choice");
    assert_eq!(
        option_names(&renamed_board, "Effort"),
        vec!["Small", "Medium", "Large"]
    );
}

#[sqlx::test]
async fn refuses_a_kind_in_a_property_patch(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/properties/{}", property_id(&board, "Link")),
        json!({ "name": "Link", "kind": "number" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a property's kind can't change",
    )
    .await;
}

#[sqlx::test]
async fn answers_conflict_for_another_property_s_name_but_allows_a_case_change(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    let size_path = format!("/properties/{}", property_id(&board, "Size"));

    let taken_rename = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &size_path,
        json!({ "name": "link" }),
        &signed_in_person.session_cookies,
    )
    .await;
    let case_change = patch_json_with_cookies(
        build_test_app(db_pool),
        &size_path,
        json!({ "name": "SIZE" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_error(
        taken_rename,
        StatusCode::CONFLICT,
        "this board already has a property with that name",
    )
    .await;
    assert_eq!(case_change.status(), StatusCode::OK);
    assert_eq!(
        property_names(&response_json(case_change).await),
        vec!["SIZE", "Link"]
    );
}

#[sqlx::test]
async fn renames_recolors_and_clears_an_option_s_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;

    let renamed = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/options/{}", option_id(&board, "Size", "Small")),
        json!({ "name": " S " }),
        &signed_in_person.session_cookies,
    )
    .await;
    let recolored = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/options/{}", option_id(&board, "Size", "Medium")),
        json!({ "color": "#EAB308" }),
        &signed_in_person.session_cookies,
    )
    .await;
    let cleared = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/options/{}", option_id(&board, "Size", "Large")),
        json!({ "color": null }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(renamed.status(), StatusCode::OK);
    assert_eq!(recolored.status(), StatusCode::OK);
    assert_eq!(cleared.status(), StatusCode::OK);
    assert_eq!(
        property(&response_json(cleared).await, "Size")["options"],
        json!([
            { "id": option_id(&board, "Size", "Small"), "name": "S", "color": "#22c55e", "position": 0 },
            { "id": option_id(&board, "Size", "Medium"), "name": "Medium", "color": "#eab308", "position": 1 },
            { "id": option_id(&board, "Size", "Large"), "name": "Large", "color": null, "position": 2 },
        ])
    );
}

#[sqlx::test]
async fn refuses_a_bad_option_change(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &signed_in_person).await;
    let small_path = format!("/options/{}", option_id(&board, "Size", "Small"));

    for (option_body, expected_status, expected_message) in [
        (
            json!({ "name": null }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "an option name is 1 to 50 characters",
        ),
        (
            json!({ "color": "#22c55" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "a color is # followed by six hex digits, like #8fae9a",
        ),
        (
            json!({ "name": "large" }),
            StatusCode::CONFLICT,
            "this property already has an option with that name",
        ),
    ] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &small_path,
            option_body,
            &signed_in_person.session_cookies,
        )
        .await;
        assert_error(response, expected_status, expected_message).await;
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_property_or_option(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_size_and_link(&db_pool, &owner).await;

    let foreign_property_patch = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/properties/{}", property_id(&board, "Size")),
        json!({ "name": "Mine" }),
        &other_person.session_cookies,
    )
    .await;
    let foreign_option_patch = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/options/{}", option_id(&board, "Size", "Small")),
        json!({ "name": "Mine" }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(foreign_property_patch.status(), StatusCode::NOT_FOUND);
    assert_eq!(foreign_option_patch.status(), StatusCode::NOT_FOUND);
}
