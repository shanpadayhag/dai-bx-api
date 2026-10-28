use axum::http::StatusCode;
use sqlx::PgPool;

use crate::support::{
    build_test_app, delete_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tasks::create::{add_task, create_group_id, find_task_named, task_id_named};

#[sqlx::test]
async fn deletes_a_task_with_its_subtree_and_closes_the_gap_in_positions(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    for task_name in ["First", "Doomed", "Third", "Fourth"] {
        add_task(&db_pool, &signed_in_person, &task_group_id, task_name, None).await;
    }
    let doomed_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Spare", None).await;
    let doomed_task_id = task_id_named(&doomed_tasks, "Doomed");
    let child_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Doomed child",
        Some(&doomed_task_id),
    )
    .await;
    let doomed_child_id = task_id_named(&child_tasks, "Doomed child");
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Doomed grandchild",
        Some(&doomed_child_id),
    )
    .await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{doomed_task_id}"),
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let remaining_tasks = response_json(response).await;
    let remaining_names_and_positions: Vec<(String, i64)> = remaining_tasks["tasks"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|task_json| {
            (
                task_json["name"].as_str().expect("a name").to_owned(),
                task_json["position"].as_i64().expect("a position"),
            )
        })
        .collect();
    assert_eq!(
        remaining_names_and_positions,
        vec![
            (String::from("First"), 0),
            (String::from("Third"), 1),
            (String::from("Fourth"), 2),
            (String::from("Spare"), 3),
        ]
    );
    assert_eq!(remaining_tasks["openCount"], 4);
    assert_eq!(find_task_named(&remaining_tasks, "Third")["position"], 1);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_or_a_deleted_task(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let owner_group_id = create_group_id(&db_pool, &owner).await;
    let owner_tasks = add_task(&db_pool, &owner, &owner_group_id, "Mine", None).await;
    let owner_task_path = format!("/tasks/{}", task_id_named(&owner_tasks, "Mine"));

    let foreign_delete = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_task_path,
        &other_person.session_cookies,
    )
    .await;
    delete_with_cookies(
        build_test_app(db_pool.clone()),
        &owner_task_path,
        &owner.session_cookies,
    )
    .await;
    let repeated_delete = delete_with_cookies(
        build_test_app(db_pool),
        &owner_task_path,
        &owner.session_cookies,
    )
    .await;

    assert_eq!(foreign_delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(repeated_delete.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn deleting_a_task_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Task", None).await;

    let response = delete_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{}", task_id_named(&group_tasks, "Task")),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
