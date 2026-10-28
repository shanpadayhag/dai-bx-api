use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, patch_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

pub async fn find_category_id(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    category_name: &str,
) -> String {
    let categories_json = response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            "/categories",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    categories_json
        .as_array()
        .expect("a list")
        .iter()
        .find(|category_json| category_json["name"] == category_name)
        .and_then(|category_json| category_json["id"].as_str())
        .expect("the category should be listed")
        .to_owned()
}

#[sqlx::test]
async fn renames_and_recolors_a_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{studies_category_id}"),
        json!({ "name": "Chinese", "color": "#B5838D" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({ "id": studies_category_id, "name": "Chinese", "color": "#b5838d" })
    );
}

#[sqlx::test]
async fn changes_only_the_fields_sent(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{work_category_id}"),
        json!({ "color": "#000000" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(
        response_json(response).await,
        json!({ "id": work_category_id, "name": "Work", "color": "#000000" })
    );
}

#[sqlx::test]
async fn refuses_renaming_to_another_category_s_name(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{work_category_id}"),
        json!({ "name": "GOALS" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn refuses_a_bad_name_or_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;

    for rejected_changes in [json!({ "name": "" }), json!({ "color": "blue" })] {
        let response = patch_json_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/categories/{work_category_id}"),
            rejected_changes,
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_category(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_category_id = find_category_id(&db_pool, &owner, "Goals").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{owner_category_id}"),
        json!({ "name": "Taken over" }),
        &other_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "not found" })
    );
}

#[sqlx::test]
async fn answers_not_found_for_an_id_that_is_not_a_uuid(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        "/categories/not-a-uuid",
        json!({ "name": "Health" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn updating_a_category_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;

    let response = patch_json_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{work_category_id}"),
        json!({ "name": "Shift" }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
