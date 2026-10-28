use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    build_test_app, get, get_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::task_groups::create::create_group;

#[sqlx::test]
async fn lists_groups_newest_first(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for group_name in ["Monday", "Tuesday", "Wednesday"] {
        create_group(&db_pool, &signed_in_person, json!({ "name": group_name })).await;
    }

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/task-groups",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let listed_group_names: Vec<String> = response_json(response)
        .await
        .as_array()
        .expect("a list")
        .iter()
        .map(|group_json| group_json["name"].as_str().expect("a name").to_owned())
        .collect();
    assert_eq!(listed_group_names, vec!["Wednesday", "Tuesday", "Monday"]);
}

#[sqlx::test]
async fn shows_nobody_else_s_groups(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    create_group(&db_pool, &owner, json!({ "name": "Private" })).await;

    let other_person_groups = response_json(
        get_with_cookies(
            build_test_app(db_pool),
            "/task-groups",
            &other_person.session_cookies,
        )
        .await,
    )
    .await;

    assert_eq!(other_person_groups, json!([]));
}

#[sqlx::test]
async fn listing_groups_needs_a_session(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/task-groups").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
