use serde_json::json;
use sqlx::PgPool;

use crate::search::{listed_values, search_for};
use crate::support::sign_up_with_authenticator;
use crate::task_groups::create::create_group;
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{add_task, task_id_named};
use crate::tracking::start::start_task;

async fn group_with_tasks(
    db_pool: &PgPool,
    signed_in_person: &crate::support::SignedInPerson,
    group_name: &str,
    task_names: &[&str],
) -> (String, serde_json::Value) {
    let task_group_id = create_group(db_pool, signed_in_person, json!({ "name": group_name }))
        .await["id"]
        .as_str()
        .expect("a group id")
        .to_owned();
    let mut group_tasks = json!({ "tasks": [] });
    for task_name in task_names {
        group_tasks = add_task(db_pool, signed_in_person, &task_group_id, task_name, None).await;
    }
    (task_group_id, group_tasks)
}

#[sqlx::test]
async fn finds_tasks_by_the_start_of_every_word_in_any_order_and_case(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    group_with_tasks(
        &db_pool,
        &signed_in_person,
        "Work",
        &["Planning review", "Review notes", "Buy fruit"],
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=REV%20plan").await;

    assert_eq!(
        listed_values(&search_results, "tasks", "name"),
        vec!["Planning review"]
    );
    assert_eq!(search_results["tasks"]["items"][0]["groupName"], "Work");
    assert_eq!(search_results["tasks"]["hasMore"], false);
}

#[sqlx::test]
async fn lists_open_tasks_before_done_ones(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (_, group_tasks) = group_with_tasks(
        &db_pool,
        &signed_in_person,
        "Work",
        &["Plan the launch", "Plan the review"],
    )
    .await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Plan the launch"),
        true,
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    assert_eq!(
        listed_values(&search_results, "tasks", "name"),
        vec!["Plan the review", "Plan the launch"]
    );
    assert_eq!(search_results["tasks"]["items"][0]["done"], false);
    assert_eq!(search_results["tasks"]["items"][1]["done"], true);
}

#[sqlx::test]
async fn names_every_parent_of_a_subtask_outermost_first(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, mut group_tasks) =
        group_with_tasks(&db_pool, &signed_in_person, "Weekly", &["Groceries"]).await;
    for (parent_name, child_name) in [
        ("Groceries", "Fruit"),
        ("Fruit", "Apples"),
        ("Apples", "Green apples"),
    ] {
        let parent_task_id = task_id_named(&group_tasks, parent_name);
        group_tasks = add_task(
            &db_pool,
            &signed_in_person,
            &task_group_id,
            child_name,
            Some(&parent_task_id),
        )
        .await;
    }

    let search_results = search_for(&db_pool, &signed_in_person, "q=green").await;

    assert_eq!(
        search_results["tasks"]["items"][0]["parentNames"],
        json!(["Groceries", "Fruit", "Apples"])
    );
    assert_eq!(search_results["tasks"]["items"][0]["groupName"], "Weekly");
}

#[sqlx::test]
async fn marks_a_running_task(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (_, group_tasks) =
        group_with_tasks(&db_pool, &signed_in_person, "Work", &["Write the plan"]).await;
    start_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Write the plan"),
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=write").await;

    assert_eq!(search_results["tasks"]["items"][0]["running"], true);
}

#[sqlx::test]
async fn says_when_more_tasks_matched_than_the_limit(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    group_with_tasks(
        &db_pool,
        &signed_in_person,
        "Work",
        &["Plan one", "Plan two", "Plan three"],
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan&kind=tasks&limit=2").await;

    assert_eq!(listed_values(&search_results, "tasks", "name").len(), 2);
    assert_eq!(search_results["tasks"]["hasMore"], true);
}

#[sqlx::test]
async fn finds_groups_with_their_open_task_count(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (_, group_tasks) = group_with_tasks(
        &db_pool,
        &signed_in_person,
        "Weekly chores",
        &["Laundry", "Dishes", "Vacuum"],
    )
    .await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Dishes"),
        true,
    )
    .await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=chore").await;

    assert_eq!(
        listed_values(&search_results, "groups", "name"),
        vec!["Weekly chores"]
    );
    assert_eq!(search_results["groups"]["items"][0]["openTaskCount"], 2);
    assert!(listed_values(&search_results, "tasks", "name").is_empty());
}

#[sqlx::test]
async fn never_finds_another_persons_tasks_or_groups(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    group_with_tasks(&db_pool, &other_person, "Planning", &["Plan the trip"]).await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    assert!(listed_values(&search_results, "tasks", "name").is_empty());
    assert!(listed_values(&search_results, "groups", "name").is_empty());
}
