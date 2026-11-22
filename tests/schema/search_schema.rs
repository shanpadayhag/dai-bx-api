use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::seed_user;

struct SeededRows {
    group_id: Uuid,
    task_id: Uuid,
    card_id: Uuid,
    event_id: Uuid,
    series_id: Uuid,
}

async fn seed_rows(db_pool: &PgPool) -> SeededRows {
    let user_id = seed_user(db_pool, "owner@example.com").await;
    sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid, Uuid)>(
        "WITH new_group AS (
             INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Weekly chores')
             RETURNING id
         ), new_task AS (
             INSERT INTO tasks (id, user_id, group_id, position, name)
             SELECT gen_random_uuid(), $1, id, 0, 'Write the plan' FROM new_group
             RETURNING id
         ), new_board AS (
             INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
             RETURNING id
         ), new_list AS (
             INSERT INTO board_lists (id, user_id, board_id, position, name)
             SELECT gen_random_uuid(), $1, id, 0, 'Doing' FROM new_board
             RETURNING id, board_id
         ), new_card AS (
             INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
             SELECT gen_random_uuid(), $1, board_id, id, 0, 'Chinese lessons', 'Review the vocabulary list'
             FROM new_list
             RETURNING id
         ), new_event AS (
             INSERT INTO calendar_events (id, user_id, kind, title, starts_at, ends_at)
             VALUES (gen_random_uuid(), $1, 'planned', 'Planning review', now(), now() + interval '1 hour')
             RETURNING id
         ), new_series AS (
             INSERT INTO event_series (id, user_id, title, time_zone, local_start, duration_minutes, repeat_rule)
             VALUES (gen_random_uuid(), $1, 'Gym session', 'Asia/Manila', '2026-11-23 18:00', 60, '{\"frequency\":\"weekly\"}')
             RETURNING id
         )
         SELECT (SELECT id FROM new_group), (SELECT id FROM new_task), (SELECT id FROM new_card),
                (SELECT id FROM new_event), (SELECT id FROM new_series)",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .map(|(group_id, task_id, card_id, event_id, series_id)| SeededRows {
        group_id,
        task_id,
        card_id,
        event_id,
        series_id,
    })
    .expect("seed rows insert should succeed")
}

async fn is_matching(db_pool: &PgPool, table_name: &str, row_id: Uuid, search_text: &str) -> bool {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT search_vector @@ to_tsquery('simple', $2) FROM {table_name} WHERE id = $1"
    )))
    .bind(row_id)
    .bind(search_text)
    .fetch_one(db_pool)
    .await
    .expect("search vector match should run")
}

async fn rename_row(
    db_pool: &PgPool,
    table_name: &str,
    column_name: &str,
    row_id: Uuid,
    new_text: &str,
) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table_name} SET {column_name} = $2 WHERE id = $1"
    )))
    .bind(row_id)
    .bind(new_text)
    .execute(db_pool)
    .await
    .expect("rename should succeed");
}

#[sqlx::test]
async fn every_searched_row_gets_a_search_vector_on_insert(db_pool: PgPool) {
    let seeded_rows = seed_rows(&db_pool).await;

    assert!(is_matching(&db_pool, "task_groups", seeded_rows.group_id, "chore:*").await);
    assert!(is_matching(&db_pool, "tasks", seeded_rows.task_id, "plan:*").await);
    assert!(is_matching(&db_pool, "cards", seeded_rows.card_id, "chin:*").await);
    assert!(is_matching(&db_pool, "cards", seeded_rows.card_id, "vocab:*").await);
    assert!(
        is_matching(
            &db_pool,
            "calendar_events",
            seeded_rows.event_id,
            "review:*"
        )
        .await
    );
    assert!(is_matching(&db_pool, "event_series", seeded_rows.series_id, "gym:*").await);
}

#[sqlx::test]
async fn search_vectors_follow_renames_and_note_edits(db_pool: PgPool) {
    let seeded_rows = seed_rows(&db_pool).await;

    rename_row(
        &db_pool,
        "task_groups",
        "name",
        seeded_rows.group_id,
        "Errands",
    )
    .await;
    rename_row(&db_pool, "tasks", "name", seeded_rows.task_id, "Buy fruit").await;
    rename_row(
        &db_pool,
        "cards",
        "notes",
        seeded_rows.card_id,
        "Practise tones",
    )
    .await;
    rename_row(
        &db_pool,
        "calendar_events",
        "title",
        seeded_rows.event_id,
        "Standup",
    )
    .await;
    rename_row(
        &db_pool,
        "event_series",
        "title",
        seeded_rows.series_id,
        "Swimming",
    )
    .await;

    assert!(is_matching(&db_pool, "task_groups", seeded_rows.group_id, "errand:*").await);
    assert!(!is_matching(&db_pool, "task_groups", seeded_rows.group_id, "chore:*").await);
    assert!(is_matching(&db_pool, "tasks", seeded_rows.task_id, "fruit:*").await);
    assert!(!is_matching(&db_pool, "tasks", seeded_rows.task_id, "plan:*").await);
    assert!(is_matching(&db_pool, "cards", seeded_rows.card_id, "tone:*").await);
    assert!(!is_matching(&db_pool, "cards", seeded_rows.card_id, "vocab:*").await);
    assert!(
        is_matching(
            &db_pool,
            "calendar_events",
            seeded_rows.event_id,
            "standup:*"
        )
        .await
    );
    assert!(is_matching(&db_pool, "event_series", seeded_rows.series_id, "swim:*").await);
}

#[sqlx::test]
async fn card_titles_weigh_more_than_card_notes(db_pool: PgPool) {
    let seeded_rows = seed_rows(&db_pool).await;

    assert!(is_matching(&db_pool, "cards", seeded_rows.card_id, "chinese:*A").await);
    assert!(!is_matching(&db_pool, "cards", seeded_rows.card_id, "review:*A").await);
    assert!(is_matching(&db_pool, "cards", seeded_rows.card_id, "review:*B").await);
}

#[sqlx::test]
async fn each_search_vector_has_a_gin_index(db_pool: PgPool) {
    let index_definitions: Vec<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = ANY($1) ORDER BY indexname",
    )
    .bind(vec![
        "calendar_events_search_vector_index",
        "cards_search_vector_index",
        "event_series_search_vector_index",
        "task_groups_search_vector_index",
        "tasks_search_vector_index",
    ])
    .fetch_all(&db_pool)
    .await
    .expect("index lookup should succeed");

    assert_eq!(index_definitions.len(), 5);
    assert!(
        index_definitions
            .iter()
            .all(|index_definition| index_definition.contains("USING gin (search_vector)"))
    );
    assert!(index_definitions[0].contains("WHERE (kind = 'planned'::text)"));
}
