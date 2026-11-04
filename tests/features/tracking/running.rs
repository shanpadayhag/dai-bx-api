use axum::http::StatusCode;
use sqlx::PgPool;

use crate::cards::create::{board_with_cards, card_id};
use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tracking::start::{add_top_level_task, start_card, start_task};

pub async fn list_running(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> serde_json::Value {
    let response = get_with_cookies(
        build_test_app(db_pool.clone()),
        "/tracking/running",
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn running_titles(running_items: &serde_json::Value) -> Vec<&str> {
    running_items
        .as_array()
        .expect("a list of running items")
        .iter()
        .map(|running_item| running_item["title"].as_str().expect("a title"))
        .collect()
}

#[sqlx::test]
async fn lists_several_running_items_newest_first(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (changelog_task_id, _) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let (review_task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Review").await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;

    start_task(&db_pool, &signed_in_person, &changelog_task_id).await;
    start_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the release"),
    )
    .await;
    start_task(&db_pool, &signed_in_person, &review_task_id).await;

    let running_items = list_running(&db_pool, &signed_in_person).await;
    assert_eq!(
        running_titles(&running_items),
        vec!["Review", "Plan the release", "Write the changelog"]
    );
    assert_eq!(running_items[1]["estimateMinutes"], serde_json::Value::Null);
}

#[sqlx::test]
async fn lists_only_the_person_s_own_running_items(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &owner, "Write the changelog").await;
    start_task(&db_pool, &owner, &task_id).await;

    let running_items = list_running(&db_pool, &other_person).await;

    assert_eq!(running_items, serde_json::json!([]));
}

#[sqlx::test]
async fn refuses_the_running_list_without_a_session(db_pool: PgPool) {
    let response = get_with_cookies(build_test_app(db_pool), "/tracking/running", "").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
