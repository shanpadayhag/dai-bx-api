use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::seed_user;

struct SeededTasks {
    user_id: Uuid,
    top_level_task_id: Uuid,
    subtask_id: Uuid,
}

async fn seed_tasks(db_pool: &PgPool) -> SeededTasks {
    let user_id = seed_user(db_pool, "owner@example.com").await;
    let (top_level_task_id, subtask_id): (Uuid, Uuid) = sqlx::query_as(
        "WITH new_group AS (
             INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Work')
             RETURNING id
         ), new_parent AS (
             INSERT INTO tasks (id, user_id, group_id, position, name)
             SELECT gen_random_uuid(), $1, id, 0, 'Write the plan' FROM new_group
             RETURNING id, group_id
         ), new_child AS (
             INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
             SELECT gen_random_uuid(), $1, group_id, id, 0, 'Outline' FROM new_parent
             RETURNING id
         )
         SELECT (SELECT id FROM new_parent), (SELECT id FROM new_child)",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed tasks insert should succeed");
    SeededTasks {
        user_id,
        top_level_task_id,
        subtask_id,
    }
}

async fn run_statement(db_pool: &PgPool, statement_sql: String) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(statement_sql))
        .execute(db_pool)
        .await
        .map(|_| ())
}

fn is_check_violation(statement_result: &Result<(), sqlx::Error>) -> bool {
    matches!(
        statement_result,
        Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some("23514")
    )
}

async fn update_task(
    db_pool: &PgPool,
    task_id: Uuid,
    assignments: &str,
) -> Result<(), sqlx::Error> {
    run_statement(
        db_pool,
        format!("UPDATE tasks SET {assignments} WHERE id = '{task_id}'::uuid"),
    )
    .await
}

async fn insert_planned_event(
    db_pool: &PgPool,
    seeded_tasks: &SeededTasks,
    task_id: Uuid,
    extra_columns: &str,
    extra_values: &str,
) -> Result<(), sqlx::Error> {
    run_statement(
        db_pool,
        format!(
            "INSERT INTO calendar_events (id, user_id, kind, title, task_id, starts_at, ends_at{extra_columns})
             VALUES (gen_random_uuid(), '{}'::uuid, 'planned', 'Write the plan', '{task_id}'::uuid,
                     '2026-11-20 08:00Z', '2026-11-20 09:00Z'{extra_values})",
            seeded_tasks.user_id
        ),
    )
    .await
}

#[sqlx::test]
async fn existing_tasks_start_normal_with_no_due_date_or_window(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;

    let (priority, due_date, window_start): (String, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT priority, due_date::text, plan_window_start::text FROM tasks WHERE id = $1",
        )
        .bind(seeded_tasks.top_level_task_id)
        .fetch_one(&db_pool)
        .await
        .expect("task should be readable");

    assert_eq!(priority, "normal");
    assert_eq!(due_date, None);
    assert_eq!(window_start, None);
}

#[sqlx::test]
async fn a_top_level_task_takes_every_planning_field(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;

    let update_result = update_task(
        &db_pool,
        seeded_tasks.top_level_task_id,
        "priority = 'high', due_date = '2026-11-21', due_time = '17:00',
         plan_window_start = '22:00', plan_window_end = '02:00'",
    )
    .await;

    assert!(update_result.is_ok(), "{update_result:?}");
}

#[sqlx::test]
async fn task_checks_refuse_bad_planning_fields(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;
    let refused_assignments = [
        "priority = 'urgent'",
        "due_time = '17:00'",
        "plan_window_start = '08:00'",
        "plan_window_end = '08:00'",
        "plan_window_start = '08:00', plan_window_end = '08:00'",
    ];

    for refused_assignment in refused_assignments {
        let update_result =
            update_task(&db_pool, seeded_tasks.top_level_task_id, refused_assignment).await;
        assert!(
            matches!(&update_result, Err(sqlx::Error::Database(_))),
            "{refused_assignment} should be refused"
        );
    }
}

#[sqlx::test]
async fn a_subtask_refuses_every_planning_field(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;
    let refused_assignments = [
        "priority = 'high'",
        "priority = 'low'",
        "due_date = '2026-11-21'",
        "plan_window_start = '08:00', plan_window_end = '10:00'",
    ];

    for refused_assignment in refused_assignments {
        let update_result =
            update_task(&db_pool, seeded_tasks.subtask_id, refused_assignment).await;
        assert!(
            is_check_violation(&update_result),
            "{refused_assignment} on a subtask should be refused"
        );
    }
}

#[sqlx::test]
async fn planning_hours_must_be_an_array(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;

    let object_result = run_statement(
        &db_pool,
        format!("UPDATE users SET planning_hours = '{{}}'::jsonb WHERE id = '{user_id}'::uuid"),
    )
    .await;
    let array_result = run_statement(
        &db_pool,
        format!(
            "UPDATE users SET planning_hours = '[null,null,null,null,null,null,null]'::jsonb
             WHERE id = '{user_id}'::uuid"
        ),
    )
    .await;

    assert!(is_check_violation(&object_result));
    assert!(array_result.is_ok(), "{array_result:?}");
}

#[sqlx::test]
async fn a_single_planned_event_can_link_a_task(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;

    let insert_result = insert_planned_event(
        &db_pool,
        &seeded_tasks,
        seeded_tasks.top_level_task_id,
        "",
        "",
    )
    .await;

    assert!(insert_result.is_ok(), "{insert_result:?}");
}

#[sqlx::test]
async fn a_series_occurrence_cannot_link_a_task(db_pool: PgPool) {
    let seeded_tasks = seed_tasks(&db_pool).await;
    let series_id: Uuid = sqlx::query_scalar(
        "INSERT INTO event_series
             (id, user_id, title, time_zone, local_start, duration_minutes, repeat_rule)
         VALUES (gen_random_uuid(), $1, 'Gym', 'Asia/Manila', '2026-11-20 08:00', 60,
                 '{\"frequency\":\"daily\",\"interval\":1,\"ends\":{\"never\":true}}'::jsonb)
         RETURNING id",
    )
    .bind(seeded_tasks.user_id)
    .fetch_one(&db_pool)
    .await
    .expect("seed series insert should succeed");

    let insert_result = insert_planned_event(
        &db_pool,
        &seeded_tasks,
        seeded_tasks.top_level_task_id,
        ", series_id, original_start",
        &format!(", '{series_id}'::uuid, '2026-11-20 00:00Z'"),
    )
    .await;

    assert!(is_check_violation(&insert_result));
}
