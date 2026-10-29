use axum::http::StatusCode;
use sqlx::PgPool;

use crate::categories::update::find_category_id;
use crate::support::{
    build_test_app, delete_with_cookies, get_with_cookies, response_json,
    sign_up_with_authenticator,
};

#[sqlx::test]
async fn deletes_a_category(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;

    let response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/categories/{studies_category_id}"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        serde_json::json!({ "status": "deleted" })
    );
    let remaining_categories = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/categories",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;
    let remaining_category_names: Vec<&str> = remaining_categories
        .as_array()
        .expect("a list")
        .iter()
        .map(|category_json| category_json["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(remaining_category_names, vec!["Goals", "Personal", "Work"]);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_deleted_category(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_category_id = find_category_id(&db_pool, &owner, "Goals").await;

    let other_person_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/categories/{owner_category_id}"),
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/categories/{owner_category_id}"),
        &owner.session_cookies,
    )
    .await;
    let second_delete_response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{owner_category_id}"),
        &owner.session_cookies,
    )
    .await;

    assert_eq!(other_person_response.status(), StatusCode::NOT_FOUND);
    assert_eq!(second_delete_response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_category_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let work_category_id = find_category_id(&db_pool, &signed_in_person, "Work").await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/categories/{work_category_id}"),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
