use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_user};

const BOARD_TABLES: [&str; 6] = [
    "boards",
    "board_lists",
    "cards",
    "board_properties",
    "property_options",
    "card_property_values",
];

async fn seed_board(db_pool: &PgPool, user_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Personal')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed board insert should succeed")
}

async fn seed_board_list(db_pool: &PgPool, user_id: Uuid, board_id: Uuid, position: i32) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO board_lists (id, user_id, board_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, $3, 'Backlog')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(position)
    .fetch_one(db_pool)
    .await
    .expect("seed board list insert should succeed")
}

async fn seed_card(db_pool: &PgPool, user_id: Uuid, board_id: Uuid, list_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Plan the week', '')
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(list_id)
    .fetch_one(db_pool)
    .await
    .expect("seed card insert should succeed")
}

async fn seed_board_property(
    db_pool: &PgPool,
    user_id: Uuid,
    board_id: Uuid,
    name: &str,
    kind: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO board_properties (id, user_id, board_id, position, name, kind)
         VALUES (gen_random_uuid(), $1, $2, 0, $3, $4)
         RETURNING id",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(name)
    .bind(kind)
    .fetch_one(db_pool)
    .await
    .expect("seed board property insert should succeed")
}

async fn seed_property_option(
    db_pool: &PgPool,
    user_id: Uuid,
    property_id: Uuid,
    name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO property_options (id, user_id, property_id, position, name, color)
         VALUES (gen_random_uuid(), $1, $2, 0, $3, '#be7e86')
         RETURNING id",
    )
    .bind(user_id)
    .bind(property_id)
    .bind(name)
    .fetch_one(db_pool)
    .await
    .expect("seed property option insert should succeed")
}

async fn insert_option_value(
    db_pool: &PgPool,
    user_id: Uuid,
    board_id: Uuid,
    card_id: Uuid,
    property_id: Uuid,
    option_id: Uuid,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, option_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(board_id)
    .bind(card_id)
    .bind(property_id)
    .bind(option_id)
    .execute(db_pool)
    .await
}

async fn seed_task_linked_to_card(db_pool: &PgPool, user_id: Uuid, card_id: Uuid) -> Uuid {
    let task_group_id: Uuid = sqlx::query_scalar(
        "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Today')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task group insert should succeed");
    sqlx::query_scalar(
        "INSERT INTO tasks (id, user_id, group_id, position, name, card_id)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Draft the outline', $3)
         RETURNING id",
    )
    .bind(user_id)
    .bind(task_group_id)
    .bind(card_id)
    .fetch_one(db_pool)
    .await
    .expect("seed linked task insert should succeed")
}

struct SeededBoard {
    user_id: Uuid,
    board_id: Uuid,
    card_id: Uuid,
    choice_property_id: Uuid,
    chosen_option_id: Uuid,
}

async fn seed_board_with_a_chosen_option(db_pool: &PgPool, email: &str) -> SeededBoard {
    let user_id = seed_user(db_pool, email).await;
    let board_id = seed_board(db_pool, user_id).await;
    let list_id = seed_board_list(db_pool, user_id, board_id, 0).await;
    let card_id = seed_card(db_pool, user_id, board_id, list_id).await;
    let choice_property_id =
        seed_board_property(db_pool, user_id, board_id, "Project", "choice").await;
    let chosen_option_id =
        seed_property_option(db_pool, user_id, choice_property_id, "Chinese").await;
    insert_option_value(
        db_pool,
        user_id,
        board_id,
        card_id,
        choice_property_id,
        chosen_option_id,
    )
    .await
    .expect("option value insert should succeed");
    SeededBoard {
        user_id,
        board_id,
        card_id,
        choice_property_id,
        chosen_option_id,
    }
}

#[sqlx::test]
async fn deleting_a_user_removes_every_board_row_they_own(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(seeded_board.user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    for board_table in BOARD_TABLES {
        assert_eq!(
            count_rows(&db_pool, board_table).await,
            0,
            "{board_table} should be empty"
        );
    }
}

#[sqlx::test]
async fn deleting_a_board_removes_its_lists_cards_properties_options_and_values(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let other_board = seed_board_with_a_chosen_option(&db_pool, "other@example.com").await;

    sqlx::query("DELETE FROM boards WHERE id = $1")
        .bind(seeded_board.board_id)
        .execute(&db_pool)
        .await
        .expect("board delete should succeed");

    for board_table in BOARD_TABLES {
        assert_eq!(
            count_rows(&db_pool, board_table).await,
            1,
            "{board_table} should keep only the other board's row"
        );
    }
    let remaining_board_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM boards")
        .fetch_all(&db_pool)
        .await
        .expect("board list should succeed");
    assert_eq!(remaining_board_ids, vec![other_board.board_id]);
}

#[sqlx::test]
async fn deleting_a_list_removes_its_cards(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let board_id = seed_board(&db_pool, user_id).await;
    let deleted_list_id = seed_board_list(&db_pool, user_id, board_id, 0).await;
    let kept_list_id = seed_board_list(&db_pool, user_id, board_id, 1).await;
    seed_card(&db_pool, user_id, board_id, deleted_list_id).await;
    let kept_card_id = seed_card(&db_pool, user_id, board_id, kept_list_id).await;

    sqlx::query("DELETE FROM board_lists WHERE id = $1")
        .bind(deleted_list_id)
        .execute(&db_pool)
        .await
        .expect("list delete should succeed");

    let remaining_card_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM cards")
        .fetch_all(&db_pool)
        .await
        .expect("card list should succeed");
    assert_eq!(remaining_card_ids, vec![kept_card_id]);
}

#[sqlx::test]
async fn refuses_a_card_in_another_boards_list(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let first_board_id = seed_board(&db_pool, user_id).await;
    let second_board_id = seed_board(&db_pool, user_id).await;
    let second_board_list_id = seed_board_list(&db_pool, user_id, second_board_id, 0).await;

    let cross_board_insert = sqlx::query(
        "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Stray', '')",
    )
    .bind(user_id)
    .bind(first_board_id)
    .bind(second_board_list_id)
    .execute(&db_pool)
    .await;

    assert!(cross_board_insert.is_err());
}

#[sqlx::test]
async fn refuses_a_list_on_another_persons_board(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_person_id = seed_user(&db_pool, "other@example.com").await;
    let other_board_id = seed_board(&db_pool, other_person_id).await;

    let foreign_board_insert = sqlx::query(
        "INSERT INTO board_lists (id, user_id, board_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Sneaky')",
    )
    .bind(owner_id)
    .bind(other_board_id)
    .execute(&db_pool)
    .await;

    assert!(foreign_board_insert.is_err());
}

#[sqlx::test]
async fn refuses_a_value_for_another_boards_property(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let second_board_id = seed_board(&db_pool, seeded_board.user_id).await;
    let other_board_property_id = seed_board_property(
        &db_pool,
        seeded_board.user_id,
        second_board_id,
        "Size",
        "choice",
    )
    .await;
    let other_board_option_id =
        seed_property_option(&db_pool, seeded_board.user_id, other_board_property_id, "S").await;

    let cross_board_value = insert_option_value(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        seeded_board.card_id,
        other_board_property_id,
        other_board_option_id,
    )
    .await;

    assert!(cross_board_value.is_err());
}

#[sqlx::test]
async fn refuses_a_value_using_another_propertys_option(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let size_property_id = seed_board_property(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        "Size",
        "choice",
    )
    .await;

    let mismatched_option_value = insert_option_value(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        seeded_board.card_id,
        size_property_id,
        seeded_board.chosen_option_id,
    )
    .await;

    assert!(mismatched_option_value.is_err());
}

#[sqlx::test]
async fn refuses_a_value_with_no_column_set_or_more_than_one(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let hours_property_id = seed_board_property(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        "Hours",
        "number",
    )
    .await;
    let rejected_value_inserts = [
        (
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id) VALUES ($1, $2, $3, $4)",
            "a value with nothing set",
        ),
        (
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, text_value, number_value)
             VALUES ($1, $2, $3, $4, 'two', 2)",
            "a value with both text and a number",
        ),
        (
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, text_value)
             VALUES ($1, $2, $3, $4, '')",
            "an empty text value",
        ),
        (
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, text_value)
             VALUES ($1, $2, $3, $4, repeat('a', 501))",
            "a 501-character text value",
        ),
        (
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, number_value)
             VALUES ($1, $2, $3, $4, 1e15)",
            "a number with 16 digits before the point",
        ),
    ];

    for (rejected_insert_sql, rule_description) in rejected_value_inserts {
        let rejected_insert = sqlx::query(rejected_insert_sql)
            .bind(seeded_board.user_id)
            .bind(seeded_board.board_id)
            .bind(seeded_board.card_id)
            .bind(hours_property_id)
            .execute(&db_pool)
            .await;
        assert!(rejected_insert.is_err(), "should refuse {rule_description}");
    }
}

#[sqlx::test]
async fn keeps_a_number_value_exactly_as_written(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let hours_property_id = seed_board_property(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        "Hours",
        "number",
    )
    .await;

    let stored_number_text: String = sqlx::query_scalar(
        "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, number_value)
         VALUES ($1, $2, $3, $4, '2.50') RETURNING number_value::text",
    )
    .bind(seeded_board.user_id)
    .bind(seeded_board.board_id)
    .bind(seeded_board.card_id)
    .bind(hours_property_id)
    .fetch_one(&db_pool)
    .await
    .expect("number value insert should succeed");

    assert_eq!(stored_number_text, "2.50");
}

#[sqlx::test]
async fn deleting_an_option_removes_only_the_values_using_it(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let kept_option_id = seed_property_option(
        &db_pool,
        seeded_board.user_id,
        seeded_board.choice_property_id,
        "Folio",
    )
    .await;
    let second_list_id =
        seed_board_list(&db_pool, seeded_board.user_id, seeded_board.board_id, 1).await;
    let second_card_id = seed_card(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        second_list_id,
    )
    .await;
    insert_option_value(
        &db_pool,
        seeded_board.user_id,
        seeded_board.board_id,
        second_card_id,
        seeded_board.choice_property_id,
        kept_option_id,
    )
    .await
    .expect("second option value insert should succeed");

    sqlx::query("DELETE FROM property_options WHERE id = $1")
        .bind(seeded_board.chosen_option_id)
        .execute(&db_pool)
        .await
        .expect("option delete should succeed");

    let remaining_value_card_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT card_id FROM card_property_values")
            .fetch_all(&db_pool)
            .await
            .expect("value list should succeed");
    assert_eq!(remaining_value_card_ids, vec![second_card_id]);
    assert_eq!(count_rows(&db_pool, "cards").await, 2);
}

#[sqlx::test]
async fn deleting_a_property_removes_its_options_and_values(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;

    sqlx::query("DELETE FROM board_properties WHERE id = $1")
        .bind(seeded_board.choice_property_id)
        .execute(&db_pool)
        .await
        .expect("property delete should succeed");

    assert_eq!(count_rows(&db_pool, "property_options").await, 0);
    assert_eq!(count_rows(&db_pool, "card_property_values").await, 0);
    assert_eq!(count_rows(&db_pool, "cards").await, 1);
}

#[sqlx::test]
async fn deleting_a_card_clears_only_the_link_on_its_tasks(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let linked_task_id =
        seed_task_linked_to_card(&db_pool, seeded_board.user_id, seeded_board.card_id).await;

    sqlx::query("DELETE FROM cards WHERE id = $1")
        .bind(seeded_board.card_id)
        .execute(&db_pool)
        .await
        .expect("card delete should succeed");

    let linked_card_id: Option<Uuid> =
        sqlx::query_scalar("SELECT card_id FROM tasks WHERE id = $1")
            .bind(linked_task_id)
            .fetch_one(&db_pool)
            .await
            .expect("the task should still exist");
    assert_eq!(linked_card_id, None);
    assert_eq!(count_rows(&db_pool, "card_property_values").await, 0);
}

#[sqlx::test]
async fn refuses_a_task_linked_to_another_persons_card(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_board = seed_board_with_a_chosen_option(&db_pool, "other@example.com").await;
    let task_group_id: Uuid = sqlx::query_scalar(
        "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Today') RETURNING id",
    )
    .bind(owner_id)
    .fetch_one(&db_pool)
    .await
    .expect("seed task group insert should succeed");

    let foreign_card_link = sqlx::query(
        "INSERT INTO tasks (id, user_id, group_id, position, name, card_id)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Sneaky', $3)",
    )
    .bind(owner_id)
    .bind(task_group_id)
    .bind(other_board.card_id)
    .execute(&db_pool)
    .await;

    assert!(foreign_card_link.is_err());
}

#[sqlx::test]
async fn refuses_board_values_outside_the_column_rules(db_pool: PgPool) {
    let seeded_board = seed_board_with_a_chosen_option(&db_pool, "owner@example.com").await;
    let rejected_inserts = [
        (
            "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, repeat('a', 101))",
            "a 101-character board name",
        ),
        (
            "INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, ' Padded')",
            "an untrimmed board name",
        ),
        (
            "INSERT INTO board_lists (id, user_id, board_id, position, name)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), -1, 'Negative')",
            "a negative list position",
        ),
        (
            "INSERT INTO board_lists (id, user_id, board_id, position, name)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), 1, '')",
            "an empty list name",
        ),
        (
            "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), (SELECT id FROM board_lists), 1, repeat('a', 501), '')",
            "a 501-character card title",
        ),
        (
            "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), (SELECT id FROM board_lists), 1, 'Long', repeat('a', 20001))",
            "notes over 20,000 characters",
        ),
        (
            "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes, due_time)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), (SELECT id FROM board_lists), 1, 'Timed', '', '18:00')",
            "a due time without a due date",
        ),
        (
            "INSERT INTO board_properties (id, user_id, board_id, position, name, kind)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), 1, 'Mood', 'emoji')",
            "an unknown property kind",
        ),
        (
            "INSERT INTO board_properties (id, user_id, board_id, position, name, kind)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM boards), 1, 'PROJECT', 'text')",
            "a second property named the same in another case",
        ),
        (
            "INSERT INTO property_options (id, user_id, property_id, position, name)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM board_properties), 1, 'CHINESE')",
            "a second option named the same in another case",
        ),
        (
            "INSERT INTO property_options (id, user_id, property_id, position, name, color)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM board_properties), 1, 'Loud', '#FF0000')",
            "an uppercase option color",
        ),
        (
            "INSERT INTO property_options (id, user_id, property_id, position, name)
             VALUES (gen_random_uuid(), $1, (SELECT id FROM board_properties), 1, repeat('a', 51))",
            "a 51-character option name",
        ),
    ];

    for (rejected_insert_sql, rule_description) in rejected_inserts {
        let rejected_insert = sqlx::query(rejected_insert_sql)
            .bind(seeded_board.user_id)
            .execute(&db_pool)
            .await;
        assert!(rejected_insert.is_err(), "should refuse {rule_description}");
    }
}
