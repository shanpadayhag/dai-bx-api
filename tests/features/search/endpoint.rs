use axum::http::{StatusCode, header};
use sqlx::PgPool;

use crate::search::{listed_values, search_for};
use crate::support::{
    build_test_app, capture_logs, get, get_with_cookies, response_json, sign_up_with_authenticator,
};
use crate::tracking::start::add_top_level_task;

#[sqlx::test]
async fn a_search_over_every_kind_returns_all_four_sections(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    for section_name in ["tasks", "groups", "cards", "events"] {
        assert_eq!(search_results[section_name]["items"], serde_json::json!([]));
        assert_eq!(search_results[section_name]["hasMore"], false);
    }
}

#[sqlx::test]
async fn a_search_for_one_kind_returns_only_that_section(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    add_top_level_task(&db_pool, &signed_in_person, "Plan the week").await;

    let search_results =
        search_for(&db_pool, &signed_in_person, "q=plan&kind=tasks&limit=50").await;

    let section_names: Vec<&String> = search_results
        .as_object()
        .expect("a results object")
        .keys()
        .collect();
    assert_eq!(section_names, vec!["tasks"]);
    assert_eq!(
        listed_values(&search_results, "tasks", "name"),
        vec!["Plan the week"]
    );
}

#[sqlx::test]
async fn shows_five_results_per_section_by_default(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    for task_number in 1..=6 {
        add_top_level_task(&db_pool, &signed_in_person, &format!("Plan {task_number}")).await;
    }

    let search_results = search_for(&db_pool, &signed_in_person, "q=plan").await;

    assert_eq!(listed_values(&search_results, "tasks", "name").len(), 5);
    assert_eq!(search_results["tasks"]["hasMore"], true);
}

#[sqlx::test]
async fn a_query_with_no_words_returns_empty_sections(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    add_top_level_task(&db_pool, &signed_in_person, "Plan the week").await;

    let search_results = search_for(&db_pool, &signed_in_person, "q=%21%21%20%26%20%3A%2A").await;

    assert_eq!(search_results["tasks"]["items"], serde_json::json!([]));
}

#[sqlx::test]
async fn refuses_each_bad_parameter_with_422(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let too_long_query = "a".repeat(201);
    for (search_parameters, expected_message) in [
        (String::from(""), "q is required"),
        (String::from("q=%20%20"), "q is required"),
        (format!("q={too_long_query}"), "q is at most 200 characters"),
        (
            String::from("q=plan&kind=boards"),
            "kind is all, tasks, groups, cards or events",
        ),
        (String::from("q=plan&limit=0"), "limit is 1 to 50"),
        (String::from("q=plan&limit=51"), "limit is 1 to 50"),
    ] {
        let response = get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/search?{search_parameters}"),
            &signed_in_person.session_cookies,
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{search_parameters}"
        );
        assert_eq!(response_json(response).await["error"], expected_message);
    }
}

#[sqlx::test]
async fn refuses_a_search_without_a_session_with_401(db_pool: PgPool) {
    let response = get(build_test_app(db_pool), "/search?q=plan").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn search_results_are_never_stored_by_the_browser(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;

    let response = get_with_cookies(
        build_test_app(db_pool),
        "/search?q=plan",
        &signed_in_person.session_cookies,
    )
    .await;

    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("a cache header"),
        "no-store"
    );
}

#[sqlx::test]
async fn the_typed_query_never_reaches_the_logs(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let (captured_logs, _subscriber_guard) = capture_logs();

    for search_parameters in ["q=secretword", "q=secretword&limit=99"] {
        get_with_cookies(
            build_test_app(db_pool.clone()),
            &format!("/search?{search_parameters}"),
            &signed_in_person.session_cookies,
        )
        .await;
    }

    let log_text = captured_logs.text();
    assert!(log_text.contains("/search"));
    assert!(!log_text.contains("secretword"));
}
