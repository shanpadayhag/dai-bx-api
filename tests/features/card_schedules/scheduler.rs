use axum::http::StatusCode;
use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::card_schedules::actions::run_due_schedules::RunDueSchedules;
use daibx_api::features::card_schedules::entities::schedule_run::DueRunTally;
use serde_json::json;
use sqlx::PgPool;

use crate::board_lists::create::{add_list, list_id};
use crate::card_schedules::create::{
    FIRST_MONDAY, board_with_review_card, create_schedule, iso_millis, once_at, read_card,
    schedule_id, weekly_review,
};
use crate::card_schedules::pause::pause_schedule;
use crate::card_schedules::reads::read_board;
use crate::card_schedules::runs::{list_runs, run_shapes};
use crate::cards::create::card_titles_by_list;
use crate::support::{
    SignedInPerson, build_test_app, delete_with_cookies, sign_up_with_authenticator,
};

async fn call_scheduler(db_pool: &PgPool, now_text: &str) -> DueRunTally {
    RunDueSchedules::new(db_pool)
        .execute(UtcInstant::parse(now_text).expect("a valid test time"))
        .await
        .expect("the scheduler call should run")
}

fn tally(ran_count: usize, failed_count: usize) -> DueRunTally {
    DueRunTally {
        ran_count,
        failed_count,
    }
}

async fn next_run_millis(
    db_pool: &PgPool,
    signed_in_person: &SignedInPerson,
    card_id: &str,
) -> Option<i64> {
    read_card(db_pool, signed_in_person, card_id).await["schedules"][0]["nextRunAt"]
        .as_str()
        .map(iso_millis)
}

#[sqlx::test]
async fn runs_a_due_schedule_once_however_often_it_is_called(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    let early_tally = call_scheduler(&db_pool, "2030-01-07T09:59:59Z").await;
    let first_tally = call_scheduler(&db_pool, "2030-01-07T10:00:20Z").await;
    let second_tally = call_scheduler(&db_pool, "2030-01-07T10:00:50Z").await;

    assert_eq!(early_tally, tally(0, 0));
    assert_eq!(first_tally, tally(1, 0));
    assert_eq!(second_tally, tally(0, 0));
    let board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    assert_eq!(card_titles_by_list(&board, "To Do"), vec!["Weekly review"]);
    let listed_runs = list_runs(
        &db_pool,
        &signed_in_person,
        schedule_id(&weekly_schedule),
        "",
    )
    .await;
    assert_eq!(
        run_shapes(&listed_runs),
        vec![(json!("onTime"), json!(true))]
    );
    assert_eq!(
        iso_millis(listed_runs[0]["ranAt"].as_str().expect("a run time")),
        iso_millis("2030-01-07T10:00:20Z")
    );
    assert_eq!(
        next_run_millis(&db_pool, &signed_in_person, &scheduled_card.card_id).await,
        Some(iso_millis("2030-01-14T10:00:00Z"))
    );
}

#[sqlx::test]
async fn catches_up_three_missed_weeks_with_one_late_run(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;

    let catch_up_tally = call_scheduler(&db_pool, "2030-01-24T00:00:00Z").await;

    assert_eq!(catch_up_tally, tally(1, 0));
    assert_eq!(
        run_shapes(
            &list_runs(
                &db_pool,
                &signed_in_person,
                schedule_id(&weekly_schedule),
                "",
            )
            .await
        ),
        vec![(json!("late"), json!(true))]
    );
    assert_eq!(
        next_run_millis(&db_pool, &signed_in_person, &scheduled_card.card_id).await,
        Some(iso_millis("2030-01-28T10:00:00Z"))
    );
}

#[sqlx::test]
async fn ends_a_once_schedule_after_its_run(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let once_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        once_at(&scheduled_card.to_do_list_id, "2030-03-01T09:00"),
    )
    .await;

    let run_tally = call_scheduler(&db_pool, "2030-03-01T09:01:00Z").await;
    let later_tally = call_scheduler(&db_pool, "2030-03-08T09:00:00Z").await;

    assert_eq!(run_tally, tally(1, 0));
    assert_eq!(later_tally, tally(0, 0));
    assert_eq!(
        run_shapes(&list_runs(&db_pool, &signed_in_person, schedule_id(&once_schedule), "",).await),
        vec![(json!("onTime"), json!(true))]
    );
    assert_eq!(
        next_run_millis(&db_pool, &signed_in_person, &scheduled_card.card_id).await,
        None
    );
}

#[sqlx::test]
async fn skips_paused_and_flagged_schedules(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let board_id = scheduled_card.board["board"]["id"]
        .as_str()
        .expect("a board id");
    let board_with_someday = add_list(&db_pool, &signed_in_person, board_id, "Someday").await;
    let someday_list_id = list_id(&board_with_someday, "Someday").to_owned();
    let paused_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    pause_schedule(
        &db_pool,
        &signed_in_person,
        schedule_id(&paused_schedule),
        "pause",
    )
    .await;
    let flagged_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&someday_list_id),
    )
    .await;
    let delete_response = delete_with_cookies(
        build_test_app(db_pool.clone()),
        &format!("/lists/{someday_list_id}"),
        &signed_in_person.session_cookies,
    )
    .await;
    assert_eq!(delete_response.status(), StatusCode::OK);

    let skipped_tally = call_scheduler(&db_pool, "2030-01-07T10:01:00Z").await;

    assert_eq!(skipped_tally, tally(0, 0));
    for skipped_schedule in [&paused_schedule, &flagged_schedule] {
        assert!(
            list_runs(
                &db_pool,
                &signed_in_person,
                schedule_id(skipped_schedule),
                "",
            )
            .await
            .is_empty()
        );
    }
    let board = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    assert_eq!(card_titles_by_list(&board, "Doing"), vec!["Weekly review"]);
}

#[sqlx::test]
async fn leaves_a_failed_run_due_for_the_next_call(db_pool: PgPool) {
    let signed_in_person = sign_up_with_authenticator(&db_pool).await;
    let scheduled_card = board_with_review_card(&db_pool, &signed_in_person).await;
    let weekly_schedule = create_schedule(
        &db_pool,
        &signed_in_person,
        &scheduled_card.card_id,
        weekly_review(&scheduled_card.to_do_list_id),
    )
    .await;
    sqlx::raw_sql(
        "CREATE FUNCTION refuse_schedule_runs() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'runs are refused'; END $$;
         CREATE TRIGGER refuse_schedule_runs BEFORE INSERT ON schedule_runs
         FOR EACH ROW EXECUTE FUNCTION refuse_schedule_runs();",
    )
    .execute(&db_pool)
    .await
    .expect("the refusing trigger should be created");

    let failed_tally = call_scheduler(&db_pool, "2030-01-07T10:00:20Z").await;
    let next_run_after_failure =
        next_run_millis(&db_pool, &signed_in_person, &scheduled_card.card_id).await;
    let board_after_failure = read_board(&db_pool, &signed_in_person, &scheduled_card.board).await;
    sqlx::raw_sql("DROP TRIGGER refuse_schedule_runs ON schedule_runs;")
        .execute(&db_pool)
        .await
        .expect("the refusing trigger should be dropped");
    let retried_tally = call_scheduler(&db_pool, "2030-01-07T10:01:00Z").await;

    assert_eq!(failed_tally, tally(0, 1));
    assert_eq!(next_run_after_failure, Some(iso_millis(FIRST_MONDAY)));
    assert_eq!(
        card_titles_by_list(&board_after_failure, "Doing"),
        vec!["Weekly review"]
    );
    assert_eq!(retried_tally, tally(1, 0));
    assert_eq!(
        run_shapes(
            &list_runs(
                &db_pool,
                &signed_in_person,
                schedule_id(&weekly_schedule),
                "",
            )
            .await
        ),
        vec![(json!("onTime"), json!(true))]
    );
}
