mod cards_and_events;
mod endpoint;
mod tasks_and_groups;

use axum::http::StatusCode;
use sqlx::PgPool;

use crate::support::{SignedInPerson, build_test_app, get_with_cookies, response_json};

pub async fn search_for(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    search_parameters: &str,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/search?{search_parameters}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

pub fn listed_values(
    search_results: &serde_json::Value,
    section_name: &str,
    field_name: &str,
) -> Vec<String> {
    search_results[section_name]["items"]
        .as_array()
        .expect("a list of items")
        .iter()
        .map(|listed_item| {
            listed_item[field_name]
                .as_str()
                .expect("a text field")
                .to_owned()
        })
        .collect()
}
