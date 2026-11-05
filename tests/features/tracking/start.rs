use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::account::time_tracking::patch_time_tracking;
use crate::boards::create::board_id;
use crate::cards::create::{board_with_cards, card_id};
use crate::categories::update::find_category_id;
use crate::support::{
    SignedInPerson, build_test_app, patch_json_with_cookies, post_json, post_json_with_cookies,
    response_json, sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, create_group_id, task_id_named};

pub async fn post_start(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    start_path: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        start_path,
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn start_task(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
) -> serde_json::Value {
    let response = post_start(
        db_pool,
        signed_in_person,
        &format!("/tasks/{task_id}/start"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

pub async fn start_card(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
) -> serde_json::Value {
    let response = post_start(
        db_pool,
        signed_in_person,
        &format!("/cards/{card_id}/start"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

pub async fn add_top_level_task(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_name: &str,
) -> (String, String) {
    let task_group_id = create_group_id(db_pool, signed_in_person).await;
    let group_tasks = add_task(db_pool, signed_in_person, &task_group_id, task_name, None).await;
    (task_id_named(&group_tasks, task_name), task_group_id)
}

#[sqlx::test]
async fn starting_a_task_opens_a_running_item_with_its_name_and_estimate(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, task_group_id) =
        add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let estimate_response = patch_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{task_id}"),
        json!({ "estimateMinutes": 45 }),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(estimate_response.status(), StatusCode::OK);

    let running_item = start_task(&db_pool, &signed_in_person, &task_id).await;

    assert!(running_item["id"].is_string());
    assert_eq!(running_item["kind"], "tracked");
    assert_eq!(running_item["title"], "Write the changelog");
    assert_eq!(running_item["categoryId"], serde_json::Value::Null);
    assert!(running_item["startsAt"].is_string());
    assert_eq!(running_item["endsAt"], serde_json::Value::Null);
    assert_eq!(running_item["finishedAt"], serde_json::Value::Null);
    assert_eq!(running_item["lastAnsweredAt"], serde_json::Value::Null);
    assert_eq!(
        running_item["source"],
        json!({ "kind": "task", "id": task_id, "groupId": task_group_id })
    );
    assert_eq!(running_item["estimateMinutes"], 45);
    assert_eq!(running_item["sourceName"], "Today");
}

#[sqlx::test]
async fn a_start_takes_the_tracked_category_from_the_settings(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let studies_category_id = find_category_id(&db_pool, &signed_in_person, "Studies").await;
    let settings_response = patch_time_tracking(
        &db_pool,
        &signed_in_person,
        json!({ "trackedCategoryId": studies_category_id }),
    )
    .await;
    assert_eq!(settings_response.status(), StatusCode::OK);
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;

    let task_item = start_task(&db_pool, &signed_in_person, &task_id).await;
    let card_item = start_card(
        &db_pool,
        &signed_in_person,
        card_id(&board, "Plan the release"),
    )
    .await;

    assert_eq!(task_item["categoryId"], studies_category_id.as_str());
    assert_eq!(card_item["categoryId"], studies_category_id.as_str());
}

#[sqlx::test]
async fn starting_a_card_opens_a_running_item_without_an_estimate(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;
    let release_card_id = card_id(&board, "Plan the release");

    let running_item = start_card(&db_pool, &signed_in_person, release_card_id).await;

    assert_eq!(running_item["kind"], "tracked");
    assert_eq!(running_item["title"], "Plan the release");
    assert_eq!(
        running_item["source"],
        json!({ "kind": "card", "id": release_card_id, "boardId": board_id(&board) })
    );
    assert_eq!(running_item["estimateMinutes"], serde_json::Value::Null);
    assert_eq!(running_item["sourceName"], "Personal");
}

#[sqlx::test]
async fn refuses_a_second_start_of_the_same_task_or_card(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let board = board_with_cards(
        &db_pool,
        &signed_in_person,
        &[("Doing", &["Plan the release"])],
    )
    .await;
    let release_card_id = card_id(&board, "Plan the release");
    start_task(&db_pool, &signed_in_person, &task_id).await;
    start_card(&db_pool, &signed_in_person, release_card_id).await;

    for start_path in [
        format!("/tasks/{task_id}/start"),
        format!("/cards/{release_card_id}/start"),
    ] {
        let response = post_start(&db_pool, &signed_in_person, &start_path).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{start_path}");
    }
}

#[sqlx::test]
async fn only_one_of_two_racing_starts_of_a_task_wins(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;
    let start_path = format!("/tasks/{task_id}/start");

    let (first_response, second_response) = tokio::join!(
        post_start(&db_pool, &signed_in_person, &start_path),
        post_start(&db_pool, &signed_in_person, &start_path),
    );

    let mut response_statuses = [first_response.status(), second_response.status()];
    response_statuses.sort();
    assert_eq!(
        response_statuses,
        [StatusCode::CREATED, StatusCode::CONFLICT]
    );
}

#[sqlx::test]
async fn refuses_to_start_a_subtask_or_a_done_task(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Release", None).await;
    let release_task_id = task_id_named(&group_tasks, "Release");
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Tag the build",
        Some(&release_task_id),
    )
    .await;
    let subtask_id = task_id_named(&group_tasks, "Tag the build");
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Shipped", None).await;
    let done_task_id = task_id_named(&group_tasks, "Shipped");
    set_task_done(&db_pool, &signed_in_person, &done_task_id, true).await;

    for refused_task_id in [subtask_id, done_task_id] {
        let response = post_start(
            &db_pool,
            &signed_in_person,
            &format!("/tasks/{refused_task_id}/start"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[sqlx::test]
async fn answers_not_found_for_another_person_s_task_or_card(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &owner, "Write the changelog").await;
    let board = board_with_cards(&db_pool, &owner, &[("Doing", &["Plan the release"])]).await;
    let release_card_id = card_id(&board, "Plan the release");

    for start_path in [
        format!("/tasks/{task_id}/start"),
        format!("/cards/{release_card_id}/start"),
        String::from("/tasks/not-a-uuid/start"),
        String::from("/cards/not-a-uuid/start"),
    ] {
        let response = post_start(&db_pool, &other_person, &start_path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{start_path}");
    }
}

#[sqlx::test]
async fn refuses_a_start_without_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_id, _) = add_top_level_task(&db_pool, &signed_in_person, "Write the changelog").await;

    let response = post_json(
        build_test_app(db_pool),
        &format!("/tasks/{task_id}/start"),
        json!({}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
