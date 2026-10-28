use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, get, get_with_cookies, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};

fn names_and_colors(categories_json: &serde_json::Value) -> Vec<(String, String)> {
    categories_json
        .as_array()
        .expect("categories should be a list")
        .iter()
        .map(|category_json| {
            (
                category_json["name"].as_str().expect("a name").to_owned(),
                category_json["color"].as_str().expect("a color").to_owned(),
            )
        })
        .collect()
}

#[sqlx::test]
async fn a_new_account_starts_with_the_four_default_categories(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/categories",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        names_and_colors(&response_json(response).await),
        vec![
            (String::from("Goals"), String::from("#7c95bd")),
            (String::from("Personal"), String::from("#3a405a")),
            (String::from("Studies"), String::from("#be7e86")),
            (String::from("Work"), String::from("#c6afa3")),
        ]
    );
}

#[sqlx::test]
async fn lists_categories_oldest_first_with_ids(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/categories",
        json!({ "name": "Health", "color": "#8fae9a" }),
        &signed_in_person.session_cookies,
    )
    .await;

    let categories_json = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/categories",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await;

    let listed_categories = categories_json.as_array().expect("a list");
    assert_eq!(listed_categories.len(), 5);
    assert_eq!(listed_categories[4]["name"], "Health");
    assert!(
        listed_categories
            .iter()
            .all(|category_json| category_json["id"].is_string())
    );
}

#[sqlx::test]
async fn shows_nobody_else_s_categories(db_pool: PgPool) {
    let first_person = sign_up_with_authenticator(&db_pool).await;
    let second_person = sign_up_with_authenticator(&db_pool).await;
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/categories",
        json!({ "name": "Private", "color": "#123456" }),
        &first_person.session_cookies,
    )
    .await;

    let second_person_categories = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/categories",
            &second_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(
        second_person_categories.as_array().expect("a list").len(),
        4
    );
}

#[sqlx::test]
async fn listing_categories_needs_a_session(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/categories").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
