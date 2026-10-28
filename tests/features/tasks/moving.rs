use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, post_json_with_cookies, response_json,
    sign_up_with_authenticator,
};
use crate::tasks::completion::set_task_done;
use crate::tasks::create::{
    add_task, add_task_chain, create_group_id, find_task_named, task_id_named,
};

async fn move_task(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_id: &str,
    move_request: serde_json::Value,
) -> axum::http::Response<axum::body::Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/tasks/{task_id}/move"),
        move_request,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn read_group_tasks(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    task_group_id: &str,
) -> serde_json::Value {
    response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/task-groups/{task_group_id}/tasks"),
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await
}

fn sibling_names_in_order(
    group_tasks: &serde_json::Value,
    parent_task_id: Option<&str>,
) -> Vec<String> {
    let mut sibling_tasks: Vec<&serde_json::Value> = group_tasks["tasks"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|task_json| task_json["parentId"].as_str() == parent_task_id)
        .collect();
    sibling_tasks.sort_by_key(|task_json| task_json["position"].as_i64());
    let positions: Vec<i64> = sibling_tasks
        .iter()
        .map(|task_json| task_json["position"].as_i64().expect("a position"))
        .collect();
    assert_eq!(
        positions,
        (0..i64::try_from(positions.len()).expect("a small list")).collect::<Vec<i64>>(),
        "sibling positions should run 0, 1, 2… with no gaps"
    );
    sibling_tasks
        .iter()
        .map(|task_json| task_json["name"].as_str().expect("a name").to_owned())
        .collect()
}

async fn seed_four_siblings(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
) -> (String, serde_json::Value) {
    let task_group_id = create_group_id(db_pool, signed_in_person).await;
    let mut group_tasks = serde_json::Value::Null;
    for task_name in ["A", "B", "C", "D"] {
        group_tasks = add_task(db_pool, signed_in_person, &task_group_id, task_name, None).await;
    }
    (task_group_id, group_tasks)
}

#[sqlx::test]
async fn reorders_a_task_to_the_first_middle_and_last_place(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, group_tasks) = seed_four_siblings(&db_pool, &signed_in_person).await;
    let task_c_id = task_id_named(&group_tasks, "C");

    let mut reordered_names = Vec::new();
    for target_position in [0, 2, 3] {
        let response = move_task(
            &db_pool,
            &signed_in_person,
            &task_c_id,
            json!({ "groupId": task_group_id, "parentId": null, "position": target_position }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let moved_tasks = response_json(response).await;
        assert_eq!(moved_tasks["affectedGroups"], json!([]));
        reordered_names.push(sibling_names_in_order(&moved_tasks, None));
    }

    assert_eq!(
        reordered_names,
        vec![
            vec!["C", "A", "B", "D"],
            vec!["A", "B", "C", "D"],
            vec!["A", "B", "D", "C"],
        ]
    );
}

#[sqlx::test]
async fn clamps_a_position_past_the_end_to_last_place(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, group_tasks) = seed_four_siblings(&db_pool, &signed_in_person).await;

    let moved_tasks = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id_named(&group_tasks, "A"),
            json!({ "groupId": task_group_id, "parentId": null, "position": 99 }),
        )
        .await,
    )
    .await;

    assert_eq!(
        sibling_names_in_order(&moved_tasks, None),
        vec!["B", "C", "D", "A"]
    );
}

#[sqlx::test]
async fn moves_a_task_under_another_parent_in_the_same_group(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, group_tasks) = seed_four_siblings(&db_pool, &signed_in_person).await;
    let task_a_id = task_id_named(&group_tasks, "A");

    let moved_tasks = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id_named(&group_tasks, "C"),
            json!({ "groupId": task_group_id, "parentId": task_a_id, "position": 0 }),
        )
        .await,
    )
    .await;

    assert_eq!(
        sibling_names_in_order(&moved_tasks, None),
        vec!["A", "B", "D"]
    );
    assert_eq!(
        sibling_names_in_order(&moved_tasks, Some(&task_a_id)),
        vec!["C"]
    );
}

#[sqlx::test]
async fn moves_a_three_level_subtree_to_another_group(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let source_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let destination_group_id = create_group_id(&db_pool, &signed_in_person).await;
    add_task(&db_pool, &signed_in_person, &source_group_id, "Stays", None).await;
    add_task_chain(&db_pool, &signed_in_person, &source_group_id, 3).await;
    add_task(
        &db_pool,
        &signed_in_person,
        &source_group_id,
        "Also stays",
        None,
    )
    .await;
    add_task(
        &db_pool,
        &signed_in_person,
        &destination_group_id,
        "Already there",
        None,
    )
    .await;
    let source_tasks = read_group_tasks(&db_pool, &signed_in_person, &source_group_id).await;

    let response = move_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&source_tasks, "Level 1"),
        json!({ "groupId": destination_group_id, "parentId": null, "position": 0 }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let left_group_tasks = response_json(response).await;
    assert_eq!(left_group_tasks["group"]["id"], source_group_id);
    assert_eq!(
        sibling_names_in_order(&left_group_tasks, None),
        vec!["Stays", "Also stays"]
    );
    assert_eq!(
        left_group_tasks["tasks"].as_array().expect("a list").len(),
        2
    );
    assert_eq!(
        left_group_tasks["affectedGroups"],
        json!([
            { "id": source_group_id, "openTaskCount": 2, "taskCount": 2 },
            { "id": destination_group_id, "openTaskCount": 4, "taskCount": 4 },
        ])
    );
    let destination_tasks =
        read_group_tasks(&db_pool, &signed_in_person, &destination_group_id).await;
    assert_eq!(
        sibling_names_in_order(&destination_tasks, None),
        vec!["Level 1", "Already there"]
    );
    let level_one_id = task_id_named(&destination_tasks, "Level 1");
    let level_two = find_task_named(&destination_tasks, "Level 2");
    assert_eq!(level_two["parentId"], level_one_id);
    assert_eq!(
        find_task_named(&destination_tasks, "Level 3")["parentId"],
        level_two["id"]
    );
}

#[sqlx::test]
async fn refuses_a_move_under_itself_or_its_own_subtask(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let deepest_task_id = add_task_chain(&db_pool, &signed_in_person, &task_group_id, 3).await;
    let group_tasks = read_group_tasks(&db_pool, &signed_in_person, &task_group_id).await;
    let top_task_id = task_id_named(&group_tasks, "Level 1");

    for forbidden_parent_id in [&top_task_id, &deepest_task_id] {
        let response = move_task(
            &db_pool,
            &signed_in_person,
            &top_task_id,
            json!({ "groupId": task_group_id, "parentId": forbidden_parent_id, "position": 0 }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await,
            json!({ "error": "a task can't move under itself or one of its own subtasks" })
        );
    }
}

#[sqlx::test]
async fn refuses_a_move_that_would_nest_deeper_than_32_levels(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let deep_task_id = add_task_chain(&db_pool, &signed_in_person, &task_group_id, 31).await;
    let pair_parent_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Pair parent",
        None,
    )
    .await;
    let pair_parent_id = task_id_named(&pair_parent_tasks, "Pair parent");
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Pair child",
        Some(&pair_parent_id),
    )
    .await;

    let response = move_task(
        &db_pool,
        &signed_in_person,
        &pair_parent_id,
        json!({ "groupId": task_group_id, "parentId": deep_task_id, "position": 0 }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "tasks nest at most 32 levels deep" })
    );
}

#[sqlx::test]
async fn moving_an_open_task_under_a_done_parent_reopens_it(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    let deepest_task_id = add_task_chain(&db_pool, &signed_in_person, &task_group_id, 2).await;
    let group_tasks = add_task(&db_pool, &signed_in_person, &task_group_id, "Loose", None).await;
    set_task_done(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "Level 1"),
        true,
    )
    .await;

    let moved_tasks = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id_named(&group_tasks, "Loose"),
            json!({ "groupId": task_group_id, "parentId": deepest_task_id, "position": 0 }),
        )
        .await,
    )
    .await;

    for task_name in ["Level 1", "Level 2", "Loose"] {
        assert_eq!(
            find_task_named(&moved_tasks, task_name)["isDone"],
            false,
            "{task_name} should be open"
        );
    }
}

#[sqlx::test]
async fn moving_a_done_task_under_a_done_parent_keeps_both_done(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let task_group_id = create_group_id(&db_pool, &signed_in_person).await;
    add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Done parent",
        None,
    )
    .await;
    let group_tasks = add_task(
        &db_pool,
        &signed_in_person,
        &task_group_id,
        "Done loose",
        None,
    )
    .await;
    for task_name in ["Done parent", "Done loose"] {
        set_task_done(
            &db_pool,
            &signed_in_person,
            &task_id_named(&group_tasks, task_name),
            true,
        )
        .await;
    }

    let moved_tasks = response_json(
        move_task(
            &db_pool,
            &signed_in_person,
            &task_id_named(&group_tasks, "Done loose"),
            json!({
                "groupId": task_group_id,
                "parentId": task_id_named(&group_tasks, "Done parent"),
                "position": 0,
            }),
        )
        .await,
    )
    .await;

    assert_eq!(find_task_named(&moved_tasks, "Done parent")["isDone"], true);
    assert_eq!(find_task_named(&moved_tasks, "Done loose")["isDone"], true);
}

#[sqlx::test]
async fn refuses_a_negative_position(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, group_tasks) = seed_four_siblings(&db_pool, &signed_in_person).await;

    let response = move_task(
        &db_pool,
        &signed_in_person,
        &task_id_named(&group_tasks, "A"),
        json!({ "groupId": task_group_id, "parentId": null, "position": -1 }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test]
async fn answers_not_found_for_someone_else_s_task_group_or_parent(db_pool: PgPool) {
    let owner = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let (owner_group_id, owner_tasks) = seed_four_siblings(&db_pool, &owner).await;
    let (other_group_id, other_tasks) = seed_four_siblings(&db_pool, &other_person).await;
    let owner_second_group_id = create_group_id(&db_pool, &owner).await;
    let owner_task_id = task_id_named(&owner_tasks, "A");

    let not_found_moves = [
        (
            task_id_named(&other_tasks, "A"),
            json!({ "groupId": owner_group_id, "parentId": null, "position": 0 }),
        ),
        (
            owner_task_id.clone(),
            json!({ "groupId": other_group_id, "parentId": null, "position": 0 }),
        ),
        (
            owner_task_id.clone(),
            json!({ "groupId": owner_group_id, "parentId": task_id_named(&other_tasks, "B"), "position": 0 }),
        ),
        (
            owner_task_id,
            json!({ "groupId": owner_second_group_id, "parentId": task_id_named(&owner_tasks, "B"), "position": 0 }),
        ),
    ];

    for (moved_task_id, move_request) in not_found_moves {
        let response = move_task(&db_pool, &owner, &moved_task_id, move_request.clone()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{move_request}");
    }
}

#[sqlx::test]
async fn moving_a_task_needs_a_session(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (task_group_id, group_tasks) = seed_four_siblings(&db_pool, &signed_in_person).await;

    let response = post_json_with_cookies(
        build_test_app(db_pool),
        &format!("/tasks/{}/move", task_id_named(&group_tasks, "A")),
        json!({ "groupId": task_group_id, "parentId": null, "position": 1 }),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
