use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::seed_user;

const WEEKEND_RULE: &str =
    r#"{"frequency":"weekly","interval":1,"weekdays":["sat","sun"],"ends":{"never":true}}"#;

async fn seed_task_group(db_pool: &PgPool) -> Uuid {
    let user_id = seed_user(db_pool, "owner@example.com").await;
    sqlx::query_scalar(
        "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Practice')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task group insert should succeed")
}

async fn seed_card_schedule(db_pool: &PgPool, block_minutes: Option<i32>) -> Uuid {
    let user_id = seed_user(db_pool, "owner@example.com").await;
    sqlx::query_scalar(
        "WITH new_board AS (
             INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
             RETURNING id
         ), new_list AS (
             INSERT INTO board_lists (id, user_id, board_id, position, name)
             SELECT gen_random_uuid(), $1, id, 0, 'To Do' FROM new_board
             RETURNING id, board_id
         ), new_card AS (
             INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
             SELECT gen_random_uuid(), $1, board_id, id, 0, 'Review', '' FROM new_list
             RETURNING id, board_id, list_id
         )
         INSERT INTO card_schedules
             (id, user_id, board_id, card_id, kind, target_list_id, time_zone, local_start,
              block_minutes)
         SELECT gen_random_uuid(), $1, board_id, id, 'once', list_id, 'Asia/Manila',
                '2026-11-20 08:00', $2
         FROM new_card
         RETURNING id",
    )
    .bind(user_id)
    .bind(block_minutes)
    .fetch_one(db_pool)
    .await
    .expect("seed card schedule insert should succeed")
}

async fn set_group_columns(
    db_pool: &PgPool,
    task_group_id: Uuid,
    assignments: &str,
) -> Result<(), sqlx::Error> {
    let update_sql = format!(
        "UPDATE task_groups SET {} WHERE id = '{task_group_id}'::uuid",
        assignments.replace("$RULE", &format!("'{WEEKEND_RULE}'::jsonb"))
    );
    sqlx::query(sqlx::AssertSqlSafe(update_sql))
        .execute(db_pool)
        .await
        .map(|_| ())
}

fn is_check_violation(update_result: &Result<(), sqlx::Error>) -> bool {
    matches!(
        update_result,
        Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some("23514")
    )
}

#[sqlx::test]
async fn a_new_group_shows_always_and_never_unchecks(db_pool: PgPool) {
    let task_group_id = seed_task_group(&db_pool).await;

    let (show_rule, uncheck_rule): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT show_rule::text, uncheck_rule::text FROM task_groups WHERE id = $1")
            .bind(task_group_id)
            .fetch_one(&db_pool)
            .await
            .expect("group should be readable");

    assert_eq!((show_rule, uncheck_rule), (None, None));
}

#[sqlx::test]
async fn accepts_a_show_rule_with_a_window_across_midnight_and_an_uncheck_rule(db_pool: PgPool) {
    let task_group_id = seed_task_group(&db_pool).await;

    let update_result = set_group_columns(
        &db_pool,
        task_group_id,
        "show_rule = $RULE, show_start_date = '2026-11-12', show_window_start = '22:00',
         show_window_end = '02:00', uncheck_rule = $RULE,
         uncheck_start_date = '2026-11-12', uncheck_time = '04:00'",
    )
    .await;

    assert!(update_result.is_ok(), "{update_result:?}");
}

#[sqlx::test]
async fn refuses_group_rules_that_do_not_hold_together(db_pool: PgPool) {
    let task_group_id = seed_task_group(&db_pool).await;

    for refused_assignments in [
        "show_rule = $RULE",
        "show_start_date = '2026-11-12'",
        "show_rule = $RULE, show_start_date = '2026-11-12', show_window_start = '06:00'",
        "show_rule = $RULE, show_start_date = '2026-11-12', show_window_start = '06:00',
         show_window_end = '06:00'",
        "show_window_start = '06:00', show_window_end = '12:00'",
        "uncheck_rule = $RULE, uncheck_start_date = '2026-11-12'",
        "uncheck_time = '04:00'",
        "show_rule = '\"weekly\"'::jsonb, show_start_date = '2026-11-12'",
    ] {
        let update_result = set_group_columns(&db_pool, task_group_id, refused_assignments).await;
        assert!(
            is_check_violation(&update_result),
            "{refused_assignments}: {update_result:?}"
        );
    }
}

#[sqlx::test]
async fn refuses_a_block_start_time_without_a_block_length(db_pool: PgPool) {
    let card_schedule_id = seed_card_schedule(&db_pool, None).await;

    let update_result =
        sqlx::query("UPDATE card_schedules SET block_local_time = '10:00' WHERE id = $1")
            .bind(card_schedule_id)
            .execute(&db_pool)
            .await
            .map(|_| ());

    assert!(is_check_violation(&update_result), "{update_result:?}");
}

#[sqlx::test]
async fn accepts_a_block_start_time_with_a_block_length(db_pool: PgPool) {
    let card_schedule_id = seed_card_schedule(&db_pool, Some(60)).await;

    let update_result =
        sqlx::query("UPDATE card_schedules SET block_local_time = '10:00' WHERE id = $1")
            .bind(card_schedule_id)
            .execute(&db_pool)
            .await;

    assert!(update_result.is_ok(), "{update_result:?}");
}
