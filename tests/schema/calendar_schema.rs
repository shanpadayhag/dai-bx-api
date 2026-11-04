use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_user};

struct SeededOwner {
    user_id: Uuid,
    category_id: Uuid,
    task_id: Uuid,
    card_id: Uuid,
}

async fn seed_owner(db_pool: &PgPool, email: &str) -> SeededOwner {
    let user_id = seed_user(db_pool, email).await;
    let category_id: Uuid = sqlx::query_scalar(
        "INSERT INTO categories (id, user_id, name, color)
         VALUES (gen_random_uuid(), $1, 'Focus', '#5b7fa6')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed category insert should succeed");
    let task_group_id: Uuid = sqlx::query_scalar(
        "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'DaiBX')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task group insert should succeed");
    let task_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tasks (id, user_id, group_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Write the changelog')
         RETURNING id",
    )
    .bind(user_id)
    .bind(task_group_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task insert should succeed");
    let board_id: Uuid = sqlx::query_scalar(
        "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed board insert should succeed");
    let list_id: Uuid = sqlx::query_scalar(
        "INSERT INTO board_lists (id, user_id, board_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Doing')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .fetch_one(db_pool)
    .await
    .expect("seed board list insert should succeed");
    let card_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Plan the release', '')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(list_id)
    .fetch_one(db_pool)
    .await
    .expect("seed card insert should succeed");
    SeededOwner {
        user_id,
        category_id,
        task_id,
        card_id,
    }
}

struct EventRow<'a> {
    kind: &'a str,
    category_id: Option<Uuid>,
    task_id: Option<Uuid>,
    card_id: Option<Uuid>,
    starts_at: &'a str,
    ends_at: Option<&'a str>,
    finished_at: Option<&'a str>,
    last_answered_at: Option<&'a str>,
}

const PLANNED_HOUR: EventRow<'static> = EventRow {
    kind: "planned",
    category_id: None,
    task_id: None,
    card_id: None,
    starts_at: "2026-11-05T09:00:00Z",
    ends_at: Some("2026-11-05T10:00:00Z"),
    finished_at: None,
    last_answered_at: None,
};

const RUNNING: EventRow<'static> = EventRow {
    kind: "tracked",
    category_id: None,
    task_id: None,
    card_id: None,
    starts_at: "2026-11-05T09:00:00Z",
    ends_at: None,
    finished_at: None,
    last_answered_at: None,
};

async fn insert_event(
    db_pool: &PgPool,
    user_id: Uuid,
    event_row: EventRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO calendar_events
             (id, user_id, kind, title, category_id, task_id, card_id,
              starts_at, ends_at, finished_at, last_answered_at)
         VALUES (gen_random_uuid(), $1, $2, 'Focus block', $3, $4, $5,
                 $6::timestamptz, $7::timestamptz, $8::timestamptz, $9::timestamptz)
         RETURNING id",
    )
    .bind(user_id)
    .bind(event_row.kind)
    .bind(event_row.category_id)
    .bind(event_row.task_id)
    .bind(event_row.card_id)
    .bind(event_row.starts_at)
    .bind(event_row.ends_at)
    .bind(event_row.finished_at)
    .bind(event_row.last_answered_at)
    .fetch_one(db_pool)
    .await
}

fn assert_refused_by_a_constraint<T: std::fmt::Debug>(
    write_result: Result<T, sqlx::Error>,
    refusal_description: &str,
) {
    let error_code = write_result
        .as_ref()
        .err()
        .and_then(|write_error| write_error.as_database_error())
        .and_then(|database_error| database_error.code())
        .map(|error_code| error_code.into_owned());
    assert!(
        error_code
            .as_deref()
            .is_some_and(|error_code| error_code.starts_with("23")),
        "{refusal_description}: expected a constraint violation, got {write_result:?}"
    );
}

async fn event_links(
    db_pool: &PgPool,
    event_id: Uuid,
) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    sqlx::query_as("SELECT category_id, task_id, card_id FROM calendar_events WHERE id = $1")
        .bind(event_id)
        .fetch_one(db_pool)
        .await
        .expect("event read should succeed")
}

#[sqlx::test]
async fn deleting_a_user_removes_their_events(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    insert_event(&db_pool, owner.user_id, PLANNED_HOUR)
        .await
        .expect("planned event insert should succeed");
    insert_event(&db_pool, other_owner.user_id, PLANNED_HOUR)
        .await
        .expect("other planned event insert should succeed");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner.user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    assert_eq!(count_rows(&db_pool, "calendar_events").await, 1);
}

#[sqlx::test]
async fn deleting_a_task_card_or_category_clears_only_that_link(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let task_event_id = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            category_id: Some(owner.category_id),
            task_id: Some(owner.task_id),
            ..RUNNING
        },
    )
    .await
    .expect("task event insert should succeed");
    let card_event_id = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            category_id: Some(owner.category_id),
            card_id: Some(owner.card_id),
            ..RUNNING
        },
    )
    .await
    .expect("card event insert should succeed");

    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(owner.task_id)
        .execute(&db_pool)
        .await
        .expect("task delete should succeed");
    assert_eq!(
        event_links(&db_pool, task_event_id).await,
        (Some(owner.category_id), None, None)
    );

    sqlx::query("DELETE FROM cards WHERE id = $1")
        .bind(owner.card_id)
        .execute(&db_pool)
        .await
        .expect("card delete should succeed");
    assert_eq!(
        event_links(&db_pool, card_event_id).await,
        (Some(owner.category_id), None, None)
    );

    sqlx::query("DELETE FROM categories WHERE id = $1")
        .bind(owner.category_id)
        .execute(&db_pool)
        .await
        .expect("category delete should succeed");
    assert_eq!(
        event_links(&db_pool, task_event_id).await,
        (None, None, None)
    );
    assert_eq!(
        event_links(&db_pool, card_event_id).await,
        (None, None, None)
    );
    assert_eq!(count_rows(&db_pool, "calendar_events").await, 2);
}

#[sqlx::test]
async fn refuses_a_second_open_event_for_the_same_task_or_card(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let task_running = EventRow {
        task_id: Some(owner.task_id),
        ..RUNNING
    };
    let card_running = EventRow {
        card_id: Some(owner.card_id),
        ..RUNNING
    };
    insert_event(&db_pool, owner.user_id, task_running)
        .await
        .expect("first task event insert should succeed");
    insert_event(&db_pool, owner.user_id, card_running)
        .await
        .expect("first card event insert should succeed");

    let second_task_event = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            task_id: Some(owner.task_id),
            ..RUNNING
        },
    )
    .await;
    let second_card_event = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            card_id: Some(owner.card_id),
            ..RUNNING
        },
    )
    .await;

    assert_refused_by_a_constraint(second_task_event, "a task should run only once");
    assert_refused_by_a_constraint(second_card_event, "a card should run only once");
}

#[sqlx::test]
async fn allows_a_new_run_once_the_earlier_one_has_ended(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            task_id: Some(owner.task_id),
            ends_at: Some("2026-11-05T09:40:00Z"),
            finished_at: Some("2026-11-05T09:55:00Z"),
            last_answered_at: Some("2026-11-05T09:40:00Z"),
            ..RUNNING
        },
    )
    .await
    .expect("finished task event insert should succeed");

    let next_run = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            task_id: Some(owner.task_id),
            starts_at: "2026-11-05T11:00:00Z",
            ..RUNNING
        },
    )
    .await;

    assert!(next_run.is_ok(), "a finished task should start again");
}

#[sqlx::test]
async fn refuses_a_planned_event_carrying_tracking_fields(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let rejected_rows = [
        (
            "a task",
            EventRow {
                task_id: Some(owner.task_id),
                ..PLANNED_HOUR
            },
        ),
        (
            "a card",
            EventRow {
                card_id: Some(owner.card_id),
                ..PLANNED_HOUR
            },
        ),
        (
            "a finish moment",
            EventRow {
                finished_at: Some("2026-11-05T10:00:00Z"),
                ..PLANNED_HOUR
            },
        ),
        (
            "an answered check-in",
            EventRow {
                last_answered_at: Some("2026-11-05T09:30:00Z"),
                ..PLANNED_HOUR
            },
        ),
        (
            "no end",
            EventRow {
                ends_at: None,
                ..PLANNED_HOUR
            },
        ),
    ];

    for (field_description, rejected_row) in rejected_rows {
        let rejected_insert = insert_event(&db_pool, owner.user_id, rejected_row).await;
        assert_refused_by_a_constraint(
            rejected_insert,
            &format!("a planned event with {field_description} should be refused"),
        );
    }
}

#[sqlx::test]
async fn refuses_a_tracked_event_with_both_a_task_and_a_card(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let both_sources = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            task_id: Some(owner.task_id),
            card_id: Some(owner.card_id),
            ..RUNNING
        },
    )
    .await;

    assert_refused_by_a_constraint(both_sources, "a task and a card together");
}

#[sqlx::test]
async fn refuses_times_out_of_order(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let rejected_rows = [
        (
            "an end at the start",
            EventRow {
                ends_at: Some("2026-11-05T09:00:00Z"),
                ..PLANNED_HOUR
            },
        ),
        (
            "an end before the start",
            EventRow {
                ends_at: Some("2026-11-05T08:00:00Z"),
                ..PLANNED_HOUR
            },
        ),
        (
            "a planned event under 15 minutes",
            EventRow {
                ends_at: Some("2026-11-05T09:14:00Z"),
                ..PLANNED_HOUR
            },
        ),
        (
            "a finish before the end",
            EventRow {
                ends_at: Some("2026-11-05T09:40:00Z"),
                finished_at: Some("2026-11-05T09:30:00Z"),
                ..RUNNING
            },
        ),
    ];

    for (rule_description, rejected_row) in rejected_rows {
        let rejected_insert = insert_event(&db_pool, owner.user_id, rejected_row).await;
        assert_refused_by_a_constraint(
            rejected_insert,
            &format!("should refuse {rule_description}"),
        );
    }
}

#[sqlx::test]
async fn keeps_a_planned_event_that_lasts_several_days(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let trip = insert_event(
        &db_pool,
        owner.user_id,
        EventRow {
            ends_at: Some("2026-11-08T15:00:00Z"),
            ..PLANNED_HOUR
        },
    )
    .await;

    assert!(trip.is_ok(), "a three-day event should be kept");
}

#[sqlx::test]
async fn refuses_links_to_another_persons_task_card_or_category(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    let rejected_rows = [
        (
            "task",
            EventRow {
                task_id: Some(other_owner.task_id),
                ..RUNNING
            },
        ),
        (
            "card",
            EventRow {
                card_id: Some(other_owner.card_id),
                ..RUNNING
            },
        ),
        (
            "category",
            EventRow {
                category_id: Some(other_owner.category_id),
                ..PLANNED_HOUR
            },
        ),
    ];

    for (link_description, rejected_row) in rejected_rows {
        let rejected_insert = insert_event(&db_pool, owner.user_id, rejected_row).await;
        assert_refused_by_a_constraint(
            rejected_insert,
            &format!("should refuse another person's {link_description}"),
        );
    }
}

#[sqlx::test]
async fn gives_new_accounts_the_time_tracking_defaults(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;

    let time_tracking_settings: (bool, i32, i32, String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT check_ins_enabled, check_in_interval_minutes, first_check_in_minutes,
                to_char(working_hours_start, 'HH24:MI'), to_char(working_hours_end, 'HH24:MI'),
                tracked_category_id
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&db_pool)
    .await
    .expect("settings read should succeed");

    assert_eq!(
        time_tracking_settings,
        (true, 30, 30, "16:00".to_owned(), "02:00".to_owned(), None)
    );
}

#[sqlx::test]
async fn refuses_check_in_intervals_outside_five_minutes_to_four_hours(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;

    for (column_name, rejected_minutes) in [
        ("check_in_interval_minutes", 4),
        ("check_in_interval_minutes", 241),
        ("first_check_in_minutes", 4),
        ("first_check_in_minutes", 241),
    ] {
        let rejected_update = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE users SET {column_name} = $1 WHERE id = $2"
        )))
        .bind(rejected_minutes)
        .bind(user_id)
        .execute(&db_pool)
        .await;
        assert_refused_by_a_constraint(
            rejected_update,
            &format!("{column_name} = {rejected_minutes} should be refused"),
        );
    }
}

#[sqlx::test]
async fn deleting_the_tracked_category_clears_the_setting(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    sqlx::query("UPDATE users SET tracked_category_id = $1 WHERE id = $2")
        .bind(owner.category_id)
        .bind(owner.user_id)
        .execute(&db_pool)
        .await
        .expect("setting the tracked category should succeed");

    sqlx::query("DELETE FROM categories WHERE id = $1")
        .bind(owner.category_id)
        .execute(&db_pool)
        .await
        .expect("category delete should succeed");

    let tracked_category_id: Option<Uuid> =
        sqlx::query_scalar("SELECT tracked_category_id FROM users WHERE id = $1")
            .bind(owner.user_id)
            .fetch_one(&db_pool)
            .await
            .expect("settings read should succeed");
    assert_eq!(tracked_category_id, None);
}

#[sqlx::test]
async fn refuses_another_persons_category_as_the_tracked_category(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;

    let foreign_setting = sqlx::query("UPDATE users SET tracked_category_id = $1 WHERE id = $2")
        .bind(other_owner.category_id)
        .bind(owner.user_id)
        .execute(&db_pool)
        .await;

    assert_refused_by_a_constraint(foreign_setting, "another person's category");
}
