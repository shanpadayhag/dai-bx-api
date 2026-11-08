use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::list_id;
use crate::card_schedules::create::{
    board_with_review_card, create_schedule, once_at, schedule_id, weekly_review,
};
use crate::card_schedules::reads::read_board;
use crate::cards::create::{add_card, card_titles_by_list};
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, get, get_with_cookies,
    post_json_with_cookies, response_json, sign_up_with_authenticator,
};

async fn post_run_now(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
) -> axum::response::Response {
    post_json_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{schedule_id}/run-now"),
        json!({}),
        &signed_in_person.session_cookies,
    )
    .await
}

async fn run_now(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
) -> serde_json::Value {
    let response = post_run_now(db_pool, signed_in_person, schedule_id).await;
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn get_runs(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
    query: &str,
) -> axum::response::Response {
    get_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{schedule_id}/runs?{query}"),
        &signed_in_person.session_cookies,
    )
    .await
}

pub async fn list_runs(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    schedule_id: &str,
    query: &str,
) -> Vec<serde_json::Value> {
    let response = get_runs(db_pool, signed_in_person, schedule_id, query).await;
    assert_eq!(response.status(), StatusCode::OK, "{query}");
    response_json(response)
        .await
        .as_array()
        .expect("a list of runs")
        .clone()
}

pub fn run_shapes(
    listed_runs: &[serde_json::Value],
) -> Vec<(serde_json::Value, serde_json::Value)> {
    listed_runs
        .iter()
        .map(|run_json| (run_json["how"].clone(), run_json["moved"].clone()))
        .collect()
}

#[sqlx::test]
async fn runs_now_by_moving_the_card_to_the_top_of_the_target_list(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    for card_title in ["Plan", "Ship"] {
        add_card(
            &db_pool,
            &signed_in_person,
            &scheduled_card.to_do_list_id,
            card_title,
        )
        .await;
    }
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    let schedule_after_run =
        run_now(&db_pool, &signed_in_person, schedule_id(&weekly_schedule)).await;

    let board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    assert_eq!(schedule_after_run, weekly_schedule);
    assert_eq!(
        card_titles_by_list(&board, "To Do"),
        vec!["Weekly review", "Plan", "Ship"]
    );
    assert!(card_titles_by_list(&board, "Doing").is_empty());
    let listed_runs = list_runs(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        "",
    )
    .await;
    assert_eq!(
        run_shapes(&listed_runs),
        vec![(json!("byHand"), json!(true))]
    );
    assert!(listed_runs[0]["id"].is_string());
    assert!(listed_runs[0]["ranAt"].is_string());
    assert_eq!(listed_runs[0]["trackedMinutes"], 0);
}

#[sqlx::test]
async fn records_a_run_without_moving_a_card_already_in_the_target_list(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    add_card(
        &db_pool,
        &signed_in_person,
        &scheduled_card.doing_list_id,
        "Ship",
    )
    .await;
    let doing_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.doing_list_id, "2030-03-01T09:00"),
    )
    .await;
    let before_board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;

    for _ in 0..2 {
        run_now(&db_pool, &signed_in_person, schedule_id(&doing_schedule)).await;
    }

    let after_board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    assert_eq!(
        card_titles_by_list(&after_board, "Doing"),
        card_titles_by_list(&before_board, "Doing")
    );
    let listed_runs = list_runs(
        &db_pool,
        &signed_in_person,
        schedule_id(&doing_schedule),
        "",
    )
    .await;
    assert_eq!(
        run_shapes(&listed_runs),
        vec![
            (json!("byHand"), json!(false)),
            (json!("byHand"), json!(false)),
        ]
    );
    assert_ne!(listed_runs[0]["id"], listed_runs[1]["id"]);
}

#[sqlx::test]
async fn refuses_to_run_a_schedule_whose_list_was_deleted(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{}", list_id(&scheduled_card.board, "To Do")),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::OK);

    let response = post_run_now(&db_pool, &signed_in_person, schedule_id(&weekly_schedule)).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        list_runs(
            &db_pool,
            &signed_in_person,
            schedule_id(&weekly_schedule),
            ""
        )
        .await
        .is_empty()
    );
}

#[sqlx::test]
async fn pages_through_runs_newest_first(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    for _ in 0..3 {
        run_now(&db_pool, &signed_in_person, schedule_id(&weekly_schedule)).await;
    }
    let wanted_schedule_id = schedule_id(&weekly_schedule);

    let all_runs = list_runs(&db_pool, &signed_in_person, wanted_schedule_id, "").await;
    let first_page = list_runs(&db_pool, &signed_in_person, wanted_schedule_id, "limit=2").await;
    let oldest_shown = first_page[1]["ranAt"].as_str().expect("a run time");
    let second_page = list_runs(
        &db_pool,
        &signed_in_person,
        wanted_schedule_id,
        &format!("limit=2&before={oldest_shown}"),
    )
    .await;

    assert_eq!(all_runs.len(), 3);
    assert!(all_runs[0]["ranAt"].as_str() >= all_runs[1]["ranAt"].as_str());
    assert_eq!(first_page, all_runs[..2]);
    assert_eq!(second_page, all_runs[2..]);
}

#[sqlx::test]
async fn refuses_a_bad_page_and_hides_someone_else_s_runs(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let other_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    let wanted_schedule_id = schedule_id(&weekly_schedule);

    for refused_query in ["limit=0", "limit=101", "limit=many", "before=yesterday"] {
        let response = get_runs(
            &db_pool,
            &signed_in_person,
            wanted_schedule_id,
            refused_query,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused_query}"
        );
    }
    for (path_schedule_id, signed_in) in [
        (wanted_schedule_id.to_owned(), &other_person),
        (uuid::Uuid::now_v7().to_string(), &signed_in_person),
        (String::from("not-a-uuid"), &signed_in_person),
    ] {
        let runs_response = get_runs(&db_pool, signed_in, &path_schedule_id, "").await;
        let run_response = post_run_now(&db_pool, signed_in, &path_schedule_id).await;
        assert_eq!(
            runs_response.status(),
            StatusCode::NOT_FOUND,
            "{path_schedule_id}"
        );
        assert_eq!(
            run_response.status(),
            StatusCode::NOT_FOUND,
            "{path_schedule_id}"
        );
    }
    let unauthenticated = get(
        build_test_app(db_pool.clone()),
        &format!("/schedules/{wanted_schedule_id}/runs"),
    )
    .await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert!(
        list_runs(&db_pool, &signed_in_person, wanted_schedule_id, "")
            .await
            .is_empty()
    );
}
