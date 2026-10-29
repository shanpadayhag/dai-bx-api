use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, build_test_app, get_with_cookies, post_json, post_json_with_cookies,
    response_json, send, sign_up_with_authenticator,
};
use crate::tasks::create::find_task_named;

async fn import_group(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    import_file: serde_json::Value,
) -> axum::http::Response<Body> {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        "/task-groups/import",
        import_file,
        &signed_in_person.session_cookies,
    )
    .await
}

async fn list_group_names(db_pool: &PgPool, signed_in_person: &SignedInPerson) -> Vec<String> {
    response_json(
        get_with_cookies(
            build_test_app(db_pool.clone()),
            "/task-groups",
            &signed_in_person.session_cookies,
        )
        .await,
    )
    .await
    .as_array()
    .expect("a list")
    .iter()
    .map(|group_json| group_json["name"].as_str().expect("a name").to_owned())
    .collect()
}

fn flat_tasks(task_count: usize) -> Vec<serde_json::Value> {
    (0..task_count)
        .map(|task_index| json!({ "name": format!("Task {}", task_index + 1), "tasks": [] }))
        .collect()
}

fn nested_chain(chain_length: usize) -> serde_json::Value {
    (0..chain_length).rev().fold(json!([]), |child_tasks, level_index| {
        json!([{ "name": format!("Level {}", level_index + 1), "tasks": child_tasks }])
    })
}

#[sqlx::test]
async fn imports_the_daily_plan_file_with_its_names_nesting_and_order(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = import_group(
        &db_pool,
        &signed_in_person,
        json!({
            "name": "Today",
            "generatedBy": "daily plan",
            "tasks": [
                {
                    "name": "Project – Card title",
                    "tasks": [
                        { "name": "First executable action", "tasks": [] },
                        { "name": "Second executable action", "tasks": [], "estimate": "45m" },
                    ],
                },
                { "name": "  Chinese – Review this week's cards  ", "tasks": [] },
            ],
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let group_tasks = response_json(response).await;
    assert_eq!(group_tasks["group"]["name"], "Today");
    assert!(group_tasks["group"].get("category").is_none());
    assert_eq!(group_tasks["openCount"], 4);
    assert_eq!(group_tasks["doneCount"], 0);
    let card_task = find_task_named(&group_tasks, "Project – Card title");
    assert_eq!(card_task["parentId"], serde_json::Value::Null);
    assert_eq!(card_task["position"], 0);
    let second_action = find_task_named(&group_tasks, "Second executable action");
    assert_eq!(second_action["parentId"], card_task["id"]);
    assert_eq!(second_action["position"], 1);
    assert_eq!(second_action["estimateMinutes"], serde_json::Value::Null);
    assert_eq!(
        find_task_named(&group_tasks, "Chinese – Review this week's cards")["position"],
        1
    );
}

#[sqlx::test]
async fn adds_a_same_named_group_alongside_the_old_one(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let today_file = json!({ "name": "Today", "tasks": flat_tasks(2) });

    import_group(&db_pool, &signed_in_person, today_file.clone()).await;
    let second_response = import_group(&db_pool, &signed_in_person, today_file).await;

    assert_eq!(second_response.status(), StatusCode::CREATED);
    assert_eq!(
        list_group_names(&db_pool, &signed_in_person).await,
        vec!["Today", "Today"]
    );
}

#[sqlx::test]
async fn imports_2000_tasks(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = import_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Big plan", "tasks": flat_tasks(2000) }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let group_tasks = response_json(response).await;
    assert_eq!(group_tasks["openCount"], 2000);
    assert_eq!(find_task_named(&group_tasks, "Task 2000")["position"], 1999);
}

#[sqlx::test]
async fn imports_32_levels_deep(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = import_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Deep", "tasks": nested_chain(32) }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn names_the_problem_and_where_it_is(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let mut task_at_1999_unnamed = flat_tasks(1999);
    task_at_1999_unnamed[1998] = json!({ "name": "   ", "tasks": [] });
    let rejected_files = [
        (json!([]), "a group file is a JSON object", ""),
        (json!({ "tasks": [] }), "the group needs a name", "name"),
        (
            json!({ "name": "  ", "tasks": [] }),
            "the group needs a name",
            "name",
        ),
        (json!({ "name": "Today" }), "tasks must be a list", "tasks"),
        (
            json!({ "name": "Today", "tasks": {} }),
            "tasks must be a list",
            "tasks",
        ),
        (
            json!({ "name": "Today", "tasks": [{ "name": "A", "tasks": [{ "tasks": [] }] }] }),
            "every task needs a name",
            "tasks[0].tasks[0].name",
        ),
        (
            json!({ "name": "Today", "tasks": [{ "name": "A" }] }),
            "tasks must be a list",
            "tasks[0].tasks",
        ),
        (
            json!({ "name": "Today", "tasks": ["just a string"] }),
            "each task is a JSON object",
            "tasks[0]",
        ),
        (
            json!({ "name": "Today", "tasks": [{ "name": "a".repeat(501), "tasks": [] }] }),
            "a task name is 1 to 500 characters",
            "tasks[0].name",
        ),
        (
            json!({ "name": "a".repeat(201), "tasks": [] }),
            "a group name is 1 to 200 characters",
            "name",
        ),
        (
            json!({ "name": "Today", "tasks": task_at_1999_unnamed }),
            "every task needs a name",
            "tasks[1998].name",
        ),
        (
            json!({ "name": "Today", "tasks": flat_tasks(2001) }),
            "a group file holds at most 2,000 tasks",
            "tasks[2000]",
        ),
        (
            json!({ "name": "Today", "tasks": nested_chain(33) }),
            "tasks nest at most 32 levels deep",
            "tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0].tasks[0]",
        ),
    ];

    for (rejected_file, expected_error, expected_path) in rejected_files {
        let response = import_group(&db_pool, &signed_in_person, rejected_file).await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{expected_error} at {expected_path}"
        );
        assert_eq!(
            response_json(response).await,
            json!({ "error": expected_error, "path": expected_path })
        );
    }
    assert!(
        list_group_names(&db_pool, &signed_in_person)
            .await
            .is_empty()
    );
}

#[sqlx::test]
async fn refuses_a_file_that_is_not_json(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let request = Request::post("/task-groups/import")
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &signed_in_person.session_cookies)
        .body(Body::from("{ not json"))
        .expect("test request should build");

    let response = send(build_test_app(db_pool), request).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "the file isn't valid JSON", "path": "" })
    );
}

#[sqlx::test]
async fn refuses_a_file_over_1_mb_with_a_json_error(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = import_group(
        &db_pool,
        &signed_in_person,
        json!({ "name": "Huge", "tasks": [{ "name": "x".repeat(1024 * 1024), "tasks": [] }] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response_json(response).await,
        json!({ "error": "the file is larger than 1 MB" })
    );
}

#[sqlx::test]
async fn importing_needs_a_session(db_pool: PgPool) {
    let response = post_json(
        build_test_app(db_pool),
        "/task-groups/import",
        json!({ "name": "Today", "tasks": [] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
