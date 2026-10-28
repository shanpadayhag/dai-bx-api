use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, post_json, post_json_with_cookies, response_json, sign_up_with_authenticator,
};

#[sqlx::test]
async fn creates_a_category_with_a_trimmed_name_and_lowercase_color(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/categories",
        json!({ "name": "  Health  ", "color": "#8FAE9A" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created_category = response_json(response).await;
    assert_eq!(created_category["name"], "Health");
    assert_eq!(created_category["color"], "#8fae9a");
    assert!(created_category["id"].is_string());
}

#[sqlx::test]
async fn refuses_a_name_already_used_in_any_letter_case(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        "/categories",
        json!({ "name": "studies", "color": "#123456" }),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "you already have a category with that name" })
    );
}

#[sqlx::test]
async fn refuses_a_name_that_is_empty_or_too_long(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for rejected_name in ["   ", &"a".repeat(51)] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/categories",
            json!({ "name": rejected_name, "color": "#123456" }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a category name is 1 to 50 characters" })
        );
    }
}

#[sqlx::test]
async fn refuses_a_color_that_is_not_six_hex_digits(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    for rejected_color in ["red", "#12345", "#12345g", "123456"] {
        let response = post_json_with_cookies(
            build_test_app(db_pool.clone()),
            "/categories",
            json!({ "name": "Health", "color": rejected_color }),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a color is # followed by six hex digits, like #8fae9a" })
        );
    }
}

#[sqlx::test]
async fn creating_a_category_needs_a_session(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool),
        "/categories",
        json!({ "name": "Health", "color": "#8fae9a" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
