use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_user};

const WEEKLY_ON_MONDAY: &str =
    r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#;

const NEW_TABLES: [&str; 4] = [
    "card_schedules",
    "schedule_runs",
    "event_series",
    "event_exceptions",
];

struct SeededOwner {
    user_id: Uuid,
    category_id: Uuid,
    board_id: Uuid,
    doing_list_id: Uuid,
    to_do_list_id: Uuid,
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
    let board_id: Uuid = sqlx::query_scalar(
        "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed board insert should succeed");
    let doing_list_id = seed_list(db_pool, user_id, board_id, 0, "Doing").await;
    let to_do_list_id = seed_list(db_pool, user_id, board_id, 1, "To Do").await;
    let card_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Weekly review', '')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(doing_list_id)
    .fetch_one(db_pool)
    .await
    .expect("seed card insert should succeed");
    SeededOwner {
        user_id,
        category_id,
        board_id,
        doing_list_id,
        to_do_list_id,
        card_id,
    }
}

async fn seed_list(
    db_pool: &PgPool,
    user_id: Uuid,
    board_id: Uuid,
    position: i32,
    list_name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO board_lists (id, user_id, board_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, $3, $4)
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(position)
    .bind(list_name)
    .fetch_one(db_pool)
    .await
    .expect("seed board list insert should succeed")
}

struct ScheduleRow<'a> {
    user_id: Uuid,
    board_id: Uuid,
    card_id: Uuid,
    kind: &'a str,
    target_list_id: Option<Uuid>,
    repeat_rule: Option<&'a str>,
    block_minutes: Option<i32>,
    block_category_id: Option<Uuid>,
}

impl SeededOwner {
    fn weekly_schedule(&self) -> ScheduleRow<'static> {
        ScheduleRow {
            user_id: self.user_id,
            board_id: self.board_id,
            card_id: self.card_id,
            kind: "repeating",
            target_list_id: Some(self.to_do_list_id),
            repeat_rule: Some(WEEKLY_ON_MONDAY),
            block_minutes: Some(60),
            block_category_id: Some(self.category_id),
        }
    }

    fn once_schedule(&self) -> ScheduleRow<'static> {
        ScheduleRow {
            kind: "once",
            repeat_rule: None,
            block_minutes: None,
            block_category_id: None,
            ..self.weekly_schedule()
        }
    }
}

async fn insert_schedule(
    db_pool: &PgPool,
    schedule_row: ScheduleRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO card_schedules
             (id, user_id, board_id, card_id, kind, target_list_id, time_zone, local_start,
              repeat_rule, block_minutes, block_category_id, next_run_at)
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'Asia/Manila', '2026-11-09T18:00:00',
                 $6::jsonb, $7, $8, '2026-11-09T10:00:00Z')
         RETURNING id",
    )
    .bind(schedule_row.user_id)
    .bind(schedule_row.board_id)
    .bind(schedule_row.card_id)
    .bind(schedule_row.kind)
    .bind(schedule_row.target_list_id)
    .bind(schedule_row.repeat_rule)
    .bind(schedule_row.block_minutes)
    .bind(schedule_row.block_category_id)
    .fetch_one(db_pool)
    .await
}

async fn insert_series(
    db_pool: &PgPool,
    user_id: Uuid,
    schedule_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO event_series
             (id, user_id, title, time_zone, local_start, duration_minutes, repeat_rule, schedule_id)
         VALUES (gen_random_uuid(), $1, 'Weekly review', 'Asia/Manila', '2026-11-09T18:00:00',
                 60, $2::jsonb, $3)
         RETURNING id",
    )
    .bind(user_id)
    .bind(WEEKLY_ON_MONDAY)
    .bind(schedule_id)
    .fetch_one(db_pool)
    .await
}

async fn insert_exception(
    db_pool: &PgPool,
    user_id: Uuid,
    series_id: Uuid,
    original_start: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO event_exceptions (series_id, user_id, original_start)
         VALUES ($1, $2, $3::timestamptz)",
    )
    .bind(series_id)
    .bind(user_id)
    .bind(original_start)
    .execute(db_pool)
    .await
    .map(|_| ())
}

struct OverrideRow<'a> {
    series_id: Option<Uuid>,
    original_start: Option<&'a str>,
    schedule_id: Option<Uuid>,
}

async fn insert_planned_event(
    db_pool: &PgPool,
    user_id: Uuid,
    override_row: OverrideRow<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO calendar_events
             (id, user_id, kind, title, starts_at, ends_at, time_zone,
              series_id, original_start, schedule_id)
         VALUES (gen_random_uuid(), $1, 'planned', 'Weekly review',
                 '2026-11-16T11:00:00Z', '2026-11-16T12:00:00Z', 'Asia/Manila',
                 $2, $3::timestamptz, $4)
         RETURNING id",
    )
    .bind(user_id)
    .bind(override_row.series_id)
    .bind(override_row.original_start)
    .bind(override_row.schedule_id)
    .fetch_one(db_pool)
    .await
}

async fn insert_run(
    db_pool: &PgPool,
    user_id: Uuid,
    schedule_id: Uuid,
    occurrence_at: &str,
    how: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO schedule_runs (id, user_id, schedule_id, occurrence_at, ran_at, how, moved)
         VALUES (gen_random_uuid(), $1, $2, $3::timestamptz, $3::timestamptz, $4, true)
         RETURNING id",
    )
    .bind(user_id)
    .bind(schedule_id)
    .bind(occurrence_at)
    .bind(how)
    .fetch_one(db_pool)
    .await
}

struct SeededSchedule {
    schedule_id: Uuid,
    series_id: Uuid,
    override_event_id: Uuid,
}

async fn seed_schedule_with_everything(db_pool: &PgPool, owner: &SeededOwner) -> SeededSchedule {
    let schedule_id = insert_schedule(db_pool, owner.weekly_schedule())
        .await
        .expect("schedule insert should succeed");
    insert_run(
        db_pool,
        owner.user_id,
        schedule_id,
        "2026-11-09T10:00:00Z",
        "on_time",
    )
    .await
    .expect("run insert should succeed");
    let series_id = insert_series(db_pool, owner.user_id, Some(schedule_id))
        .await
        .expect("series insert should succeed");
    insert_exception(db_pool, owner.user_id, series_id, "2026-11-16T10:00:00Z")
        .await
        .expect("exception insert should succeed");
    let override_event_id = insert_planned_event(
        db_pool,
        owner.user_id,
        OverrideRow {
            series_id: Some(series_id),
            original_start: Some("2026-11-16T10:00:00Z"),
            schedule_id: Some(schedule_id),
        },
    )
    .await
    .expect("override event insert should succeed");
    SeededSchedule {
        schedule_id,
        series_id,
        override_event_id,
    }
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

async fn delete_row(db_pool: &PgPool, table_name: &str, row_id: Uuid) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM {table_name} WHERE id = $1"
    )))
    .bind(row_id)
    .execute(db_pool)
    .await
    .expect("row delete should succeed");
}

#[sqlx::test]
async fn deleting_a_user_removes_every_new_row(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    seed_schedule_with_everything(&db_pool, &owner).await;
    seed_schedule_with_everything(&db_pool, &other_owner).await;

    delete_row(&db_pool, "users", owner.user_id).await;

    for new_table in NEW_TABLES {
        assert_eq!(
            count_rows(&db_pool, new_table).await,
            1,
            "{new_table} should keep only the other person's row"
        );
    }
    assert_eq!(count_rows(&db_pool, "calendar_events").await, 1);
}

#[sqlx::test]
async fn deleting_a_series_removes_its_overrides_and_exceptions(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let seeded_schedule = seed_schedule_with_everything(&db_pool, &owner).await;
    insert_planned_event(
        &db_pool,
        owner.user_id,
        OverrideRow {
            series_id: None,
            original_start: None,
            schedule_id: None,
        },
    )
    .await
    .expect("plain planned event insert should succeed");

    delete_row(&db_pool, "event_series", seeded_schedule.series_id).await;

    assert_eq!(count_rows(&db_pool, "event_exceptions").await, 0);
    assert_eq!(count_rows(&db_pool, "calendar_events").await, 1);
    assert_eq!(count_rows(&db_pool, "card_schedules").await, 1);
}

#[sqlx::test]
async fn deleting_a_card_removes_its_schedules_and_their_runs(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let seeded_schedule = seed_schedule_with_everything(&db_pool, &owner).await;

    delete_row(&db_pool, "cards", owner.card_id).await;

    assert_eq!(count_rows(&db_pool, "card_schedules").await, 0);
    assert_eq!(count_rows(&db_pool, "schedule_runs").await, 0);
    let series_schedule_id: Option<Uuid> =
        sqlx::query_scalar("SELECT schedule_id FROM event_series WHERE id = $1")
            .bind(seeded_schedule.series_id)
            .fetch_one(&db_pool)
            .await
            .expect("series read should succeed");
    assert_eq!(series_schedule_id, None);
    assert_eq!(count_rows(&db_pool, "calendar_events").await, 1);
}

#[sqlx::test]
async fn deleting_a_board_removes_its_schedules(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    seed_schedule_with_everything(&db_pool, &owner).await;

    delete_row(&db_pool, "boards", owner.board_id).await;

    assert_eq!(count_rows(&db_pool, "card_schedules").await, 0);
    assert_eq!(count_rows(&db_pool, "schedule_runs").await, 0);
    assert_eq!(count_rows(&db_pool, "event_series").await, 1);
}

#[sqlx::test]
async fn deleting_a_target_list_clears_only_the_target(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let seeded_schedule = seed_schedule_with_everything(&db_pool, &owner).await;

    delete_row(&db_pool, "board_lists", owner.to_do_list_id).await;

    let (target_list_id, card_id, board_id, block_category_id): (
        Option<Uuid>,
        Uuid,
        Uuid,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT target_list_id, card_id, board_id, block_category_id
         FROM card_schedules WHERE id = $1",
    )
    .bind(seeded_schedule.schedule_id)
    .fetch_one(&db_pool)
    .await
    .expect("schedule read should succeed");
    assert_eq!(
        (target_list_id, card_id, board_id, block_category_id),
        (None, owner.card_id, owner.board_id, Some(owner.category_id))
    );
    assert_eq!(count_rows(&db_pool, "schedule_runs").await, 1);
}

#[sqlx::test]
async fn deleting_a_schedule_clears_it_from_its_block_series_and_event(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let seeded_schedule = seed_schedule_with_everything(&db_pool, &owner).await;

    delete_row(&db_pool, "card_schedules", seeded_schedule.schedule_id).await;

    let series_schedule_id: Option<Uuid> =
        sqlx::query_scalar("SELECT schedule_id FROM event_series WHERE id = $1")
            .bind(seeded_schedule.series_id)
            .fetch_one(&db_pool)
            .await
            .expect("series read should succeed");
    let (event_schedule_id, event_series_id): (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT schedule_id, series_id FROM calendar_events WHERE id = $1")
            .bind(seeded_schedule.override_event_id)
            .fetch_one(&db_pool)
            .await
            .expect("event read should succeed");
    assert_eq!(series_schedule_id, None);
    assert_eq!(
        (event_schedule_id, event_series_id),
        (None, Some(seeded_schedule.series_id))
    );
    assert_eq!(count_rows(&db_pool, "schedule_runs").await, 0);
}

#[sqlx::test]
async fn deleting_the_block_category_clears_only_the_category(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let seeded_schedule = seed_schedule_with_everything(&db_pool, &owner).await;

    delete_row(&db_pool, "categories", owner.category_id).await;

    let (block_minutes, block_category_id): (Option<i32>, Option<Uuid>) =
        sqlx::query_as("SELECT block_minutes, block_category_id FROM card_schedules WHERE id = $1")
            .bind(seeded_schedule.schedule_id)
            .fetch_one(&db_pool)
            .await
            .expect("schedule read should succeed");
    assert_eq!((block_minutes, block_category_id), (Some(60), None));
}

#[sqlx::test]
async fn refuses_a_second_scheduled_run_of_an_occurrence_but_not_by_hand(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let schedule_id = insert_schedule(&db_pool, owner.weekly_schedule())
        .await
        .expect("schedule insert should succeed");
    let occurrence_at = "2026-11-09T10:00:00Z";
    insert_run(
        &db_pool,
        owner.user_id,
        schedule_id,
        occurrence_at,
        "on_time",
    )
    .await
    .expect("first run insert should succeed");

    let late_retry = insert_run(&db_pool, owner.user_id, schedule_id, occurrence_at, "late").await;
    assert_refused_by_a_constraint(late_retry, "should refuse a second run of one occurrence");

    for _ in 0..2 {
        insert_run(
            &db_pool,
            owner.user_id,
            schedule_id,
            occurrence_at,
            "by_hand",
        )
        .await
        .expect("by-hand run insert should succeed");
    }
    assert_eq!(count_rows(&db_pool, "schedule_runs").await, 3);
}

#[sqlx::test]
async fn refuses_a_run_with_an_unknown_kind(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let schedule_id = insert_schedule(&db_pool, owner.weekly_schedule())
        .await
        .expect("schedule insert should succeed");

    let unknown_run = insert_run(
        &db_pool,
        owner.user_id,
        schedule_id,
        "2026-11-09T10:00:00Z",
        "early",
    )
    .await;

    assert_refused_by_a_constraint(unknown_run, "should refuse an unknown run kind");
}

#[sqlx::test]
async fn refuses_a_once_schedule_with_a_rule_and_a_repeating_one_without(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    insert_schedule(&db_pool, owner.once_schedule())
        .await
        .expect("once schedule insert should succeed");
    insert_schedule(&db_pool, owner.weekly_schedule())
        .await
        .expect("repeating schedule insert should succeed");

    let once_with_rule = insert_schedule(
        &db_pool,
        ScheduleRow {
            repeat_rule: Some(WEEKLY_ON_MONDAY),
            ..owner.once_schedule()
        },
    )
    .await;
    assert_refused_by_a_constraint(once_with_rule, "should refuse a once schedule with a rule");

    let repeating_without_rule = insert_schedule(
        &db_pool,
        ScheduleRow {
            repeat_rule: None,
            ..owner.weekly_schedule()
        },
    )
    .await;
    assert_refused_by_a_constraint(
        repeating_without_rule,
        "should refuse a repeating schedule without a rule",
    );
}

#[sqlx::test]
async fn refuses_a_short_block_or_a_block_category_without_a_block(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let short_block = insert_schedule(
        &db_pool,
        ScheduleRow {
            block_minutes: Some(10),
            ..owner.weekly_schedule()
        },
    )
    .await;
    assert_refused_by_a_constraint(short_block, "should refuse a block under 15 minutes");

    let category_without_block = insert_schedule(
        &db_pool,
        ScheduleRow {
            block_minutes: None,
            ..owner.weekly_schedule()
        },
    )
    .await;
    assert_refused_by_a_constraint(
        category_without_block,
        "should refuse a block category with no block",
    );
}

#[sqlx::test]
async fn refuses_an_override_that_names_only_its_series_or_only_its_start(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let series_id = insert_series(&db_pool, owner.user_id, None)
        .await
        .expect("series insert should succeed");

    let series_only = insert_planned_event(
        &db_pool,
        owner.user_id,
        OverrideRow {
            series_id: Some(series_id),
            original_start: None,
            schedule_id: None,
        },
    )
    .await;
    assert_refused_by_a_constraint(series_only, "should refuse an override with no start");

    let start_only = insert_planned_event(
        &db_pool,
        owner.user_id,
        OverrideRow {
            series_id: None,
            original_start: Some("2026-11-16T10:00:00Z"),
            schedule_id: None,
        },
    )
    .await;
    assert_refused_by_a_constraint(start_only, "should refuse a start with no series");
}

#[sqlx::test]
async fn refuses_a_second_override_of_the_same_occurrence(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let series_id = insert_series(&db_pool, owner.user_id, None)
        .await
        .expect("series insert should succeed");
    let same_occurrence = || OverrideRow {
        series_id: Some(series_id),
        original_start: Some("2026-11-16T10:00:00Z"),
        schedule_id: None,
    };
    insert_planned_event(&db_pool, owner.user_id, same_occurrence())
        .await
        .expect("first override insert should succeed");

    let second_override = insert_planned_event(&db_pool, owner.user_id, same_occurrence()).await;

    assert_refused_by_a_constraint(second_override, "should refuse a second override");
}

#[sqlx::test]
async fn refuses_keys_to_another_persons_card_list_category_or_series(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    let other_series_id = insert_series(&db_pool, other_owner.user_id, None)
        .await
        .expect("other series insert should succeed");
    let other_schedule_id = insert_schedule(&db_pool, other_owner.weekly_schedule())
        .await
        .expect("other schedule insert should succeed");

    let rejected_schedules = [
        (
            "card",
            ScheduleRow {
                card_id: other_owner.card_id,
                ..owner.weekly_schedule()
            },
        ),
        (
            "board",
            ScheduleRow {
                board_id: other_owner.board_id,
                ..owner.weekly_schedule()
            },
        ),
        (
            "list",
            ScheduleRow {
                target_list_id: Some(other_owner.to_do_list_id),
                ..owner.weekly_schedule()
            },
        ),
        (
            "category",
            ScheduleRow {
                block_category_id: Some(other_owner.category_id),
                ..owner.weekly_schedule()
            },
        ),
    ];
    for (key_description, rejected_schedule) in rejected_schedules {
        let rejected_insert = insert_schedule(&db_pool, rejected_schedule).await;
        assert_refused_by_a_constraint(
            rejected_insert,
            &format!("should refuse another person's {key_description}"),
        );
    }

    let other_series_override = insert_planned_event(
        &db_pool,
        owner.user_id,
        OverrideRow {
            series_id: Some(other_series_id),
            original_start: Some("2026-11-16T10:00:00Z"),
            schedule_id: None,
        },
    )
    .await;
    assert_refused_by_a_constraint(
        other_series_override,
        "should refuse another person's series",
    );

    let other_schedule_event = insert_planned_event(
        &db_pool,
        owner.user_id,
        OverrideRow {
            series_id: None,
            original_start: None,
            schedule_id: Some(other_schedule_id),
        },
    )
    .await;
    assert_refused_by_a_constraint(
        other_schedule_event,
        "should refuse another person's schedule on an event",
    );

    let other_schedule_series =
        insert_series(&db_pool, owner.user_id, Some(other_schedule_id)).await;
    assert_refused_by_a_constraint(
        other_schedule_series,
        "should refuse another person's schedule on a series",
    );

    let other_series_exception = insert_exception(
        &db_pool,
        owner.user_id,
        other_series_id,
        "2026-11-16T10:00:00Z",
    )
    .await;
    assert_refused_by_a_constraint(
        other_series_exception,
        "should refuse an exception on another person's series",
    );

    let other_schedule_run = insert_run(
        &db_pool,
        owner.user_id,
        other_schedule_id,
        "2026-11-09T10:00:00Z",
        "on_time",
    )
    .await;
    assert_refused_by_a_constraint(
        other_schedule_run,
        "should refuse a run on another person's schedule",
    );
}

#[sqlx::test]
async fn refuses_a_target_list_from_another_board_of_the_same_person(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let second_board_id: Uuid = sqlx::query_scalar(
        "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Home')
         RETURNING id",
    )
    .bind(owner.user_id)
    .fetch_one(&db_pool)
    .await
    .expect("second board insert should succeed");
    let second_board_list_id =
        seed_list(&db_pool, owner.user_id, second_board_id, 0, "Later").await;

    let cross_board_target = insert_schedule(
        &db_pool,
        ScheduleRow {
            target_list_id: Some(second_board_list_id),
            ..owner.weekly_schedule()
        },
    )
    .await;

    assert_refused_by_a_constraint(
        cross_board_target,
        "should refuse a target list on another board",
    );
    assert_ne!(owner.doing_list_id, second_board_list_id);
}

#[sqlx::test]
async fn starts_every_account_on_the_device_time_zone(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;

    let calendar_time_zone: Option<String> =
        sqlx::query_scalar("SELECT calendar_time_zone FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&db_pool)
            .await
            .expect("user read should succeed");

    assert_eq!(calendar_time_zone, None);
}
