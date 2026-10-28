use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_user};

const DEFAULT_CATEGORY_BACKFILL_SQL: &str =
    include_str!("../../migrations/20261028090100_backfill_default_categories.sql");

async fn seed_category(db_pool: &PgPool, user_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO categories (id, user_id, name, color) VALUES (gen_random_uuid(), $1, $2, '#7c95bd')
         RETURNING id",
    )
    .bind(user_id)
    .bind(name)
    .fetch_one(db_pool)
    .await
    .expect("seed category insert should succeed")
}

async fn seed_task_group(db_pool: &PgPool, user_id: Uuid, category_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO task_groups (id, user_id, name, category_id) VALUES (gen_random_uuid(), $1, 'Today', $2)
         RETURNING id",
    )
    .bind(user_id)
    .bind(category_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task group insert should succeed")
}

async fn seed_task(
    db_pool: &PgPool,
    user_id: Uuid,
    task_group_id: Uuid,
    parent_task_id: Option<Uuid>,
    position: i32,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, $3, $4, 'Write the migration')
         RETURNING id",
    )
    .bind(user_id)
    .bind(task_group_id)
    .bind(parent_task_id)
    .bind(position)
    .fetch_one(db_pool)
    .await
    .expect("seed task insert should succeed")
}

async fn find_task_group_ids(db_pool: &PgPool, task_ids: &[Uuid]) -> Vec<Uuid> {
    sqlx::query_scalar("SELECT group_id FROM tasks WHERE id = ANY($1) ORDER BY id")
        .bind(task_ids)
        .fetch_all(db_pool)
        .await
        .expect("task group lookup should succeed")
}

#[sqlx::test]
async fn deleting_a_category_clears_only_the_category_of_its_groups(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let category_id = seed_category(&db_pool, user_id, "Studies").await;
    let task_group_id = seed_task_group(&db_pool, user_id, Some(category_id)).await;

    sqlx::query("DELETE FROM categories WHERE id = $1")
        .bind(category_id)
        .execute(&db_pool)
        .await
        .expect("category delete should succeed");

    let (remaining_category_id, remaining_user_id): (Option<Uuid>, Uuid) =
        sqlx::query_as("SELECT category_id, user_id FROM task_groups WHERE id = $1")
            .bind(task_group_id)
            .fetch_one(&db_pool)
            .await
            .expect("the group should still exist");
    assert_eq!(remaining_category_id, None);
    assert_eq!(remaining_user_id, user_id);
}

#[sqlx::test]
async fn refuses_a_group_using_another_persons_category(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_person_id = seed_user(&db_pool, "other@example.com").await;
    let other_category_id = seed_category(&db_pool, other_person_id, "Work").await;

    let foreign_category_insert = sqlx::query(
        "INSERT INTO task_groups (id, user_id, name, category_id) VALUES (gen_random_uuid(), $1, 'Mine', $2)",
    )
    .bind(owner_id)
    .bind(other_category_id)
    .execute(&db_pool)
    .await;

    assert!(foreign_category_insert.is_err());
}

#[sqlx::test]
async fn refuses_a_subtask_in_a_different_group_from_its_parent(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let first_group_id = seed_task_group(&db_pool, user_id, None).await;
    let second_group_id = seed_task_group(&db_pool, user_id, None).await;
    let parent_task_id = seed_task(&db_pool, user_id, first_group_id, None, 0).await;

    let cross_group_insert = sqlx::query(
        "INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, $3, 0, 'Stray')",
    )
    .bind(user_id)
    .bind(second_group_id)
    .bind(parent_task_id)
    .execute(&db_pool)
    .await;

    assert!(cross_group_insert.is_err());
}

#[sqlx::test]
async fn refuses_a_task_in_another_persons_group(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_person_id = seed_user(&db_pool, "other@example.com").await;
    let other_group_id = seed_task_group(&db_pool, other_person_id, None).await;

    let foreign_group_insert = sqlx::query(
        "INSERT INTO tasks (id, user_id, group_id, position, name)
         VALUES (gen_random_uuid(), $1, $2, 0, 'Sneaky')",
    )
    .bind(owner_id)
    .bind(other_group_id)
    .execute(&db_pool)
    .await;

    assert!(foreign_group_insert.is_err());
}

#[sqlx::test]
async fn deleting_a_task_removes_its_whole_subtree_and_nothing_else(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let task_group_id = seed_task_group(&db_pool, user_id, None).await;
    let parent_task_id = seed_task(&db_pool, user_id, task_group_id, None, 0).await;
    let child_task_id = seed_task(&db_pool, user_id, task_group_id, Some(parent_task_id), 0).await;
    seed_task(&db_pool, user_id, task_group_id, Some(child_task_id), 0).await;
    let sibling_task_id = seed_task(&db_pool, user_id, task_group_id, None, 1).await;

    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(parent_task_id)
        .execute(&db_pool)
        .await
        .expect("task delete should succeed");

    let remaining_task_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tasks")
        .fetch_all(&db_pool)
        .await
        .expect("task list should succeed");
    assert_eq!(remaining_task_ids, vec![sibling_task_id]);
}

#[sqlx::test]
async fn moving_a_task_to_another_group_carries_its_subtree_along(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let source_group_id = seed_task_group(&db_pool, user_id, None).await;
    let destination_group_id = seed_task_group(&db_pool, user_id, None).await;
    let moved_task_id = seed_task(&db_pool, user_id, source_group_id, None, 0).await;
    let child_task_id = seed_task(&db_pool, user_id, source_group_id, Some(moved_task_id), 0).await;
    let grandchild_task_id =
        seed_task(&db_pool, user_id, source_group_id, Some(child_task_id), 0).await;

    sqlx::query("UPDATE tasks SET group_id = $1 WHERE id = $2")
        .bind(destination_group_id)
        .bind(moved_task_id)
        .execute(&db_pool)
        .await
        .expect("moving the root task should succeed");

    assert_eq!(
        find_task_group_ids(
            &db_pool,
            &[moved_task_id, child_task_id, grandchild_task_id]
        )
        .await,
        vec![destination_group_id; 3]
    );
}

#[sqlx::test]
async fn deleting_a_group_removes_its_tasks(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let task_group_id = seed_task_group(&db_pool, user_id, None).await;
    let parent_task_id = seed_task(&db_pool, user_id, task_group_id, None, 0).await;
    seed_task(&db_pool, user_id, task_group_id, Some(parent_task_id), 0).await;

    sqlx::query("DELETE FROM task_groups WHERE id = $1")
        .bind(task_group_id)
        .execute(&db_pool)
        .await
        .expect("group delete should succeed");

    assert_eq!(count_rows(&db_pool, "tasks").await, 0);
}

#[sqlx::test]
async fn deleting_a_user_removes_their_categories_groups_and_tasks(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let category_id = seed_category(&db_pool, user_id, "Goals").await;
    let task_group_id = seed_task_group(&db_pool, user_id, Some(category_id)).await;
    let parent_task_id = seed_task(&db_pool, user_id, task_group_id, None, 0).await;
    seed_task(&db_pool, user_id, task_group_id, Some(parent_task_id), 0).await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    for user_owned_table in ["categories", "task_groups", "tasks"] {
        assert_eq!(
            count_rows(&db_pool, user_owned_table).await,
            0,
            "{user_owned_table} should be empty after the user is deleted"
        );
    }
}

#[sqlx::test]
async fn refuses_a_second_category_with_the_same_name_in_any_case(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    seed_category(&db_pool, user_id, "Studies").await;

    let duplicate_insert = sqlx::query(
        "INSERT INTO categories (id, user_id, name, color) VALUES (gen_random_uuid(), $1, 'studies', '#be7e86')",
    )
    .bind(user_id)
    .execute(&db_pool)
    .await;

    assert!(duplicate_insert.is_err());
}

#[sqlx::test]
async fn refuses_values_outside_the_column_rules(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let task_group_id = seed_task_group(&db_pool, user_id, None).await;
    let rejected_inserts = [
        (
            "INSERT INTO categories (id, user_id, name, color) VALUES (gen_random_uuid(), $1, 'Loud', '#FF0000')",
            "an uppercase color",
        ),
        (
            "INSERT INTO categories (id, user_id, name, color) VALUES (gen_random_uuid(), $1, ' Padded', '#ff0000')",
            "an untrimmed category name",
        ),
        (
            "INSERT INTO categories (id, user_id, name, color) VALUES (gen_random_uuid(), $1, repeat('a', 51), '#ff0000')",
            "a 51-character category name",
        ),
        (
            "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, '')",
            "an empty group name",
        ),
        (
            "INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, repeat('a', 201))",
            "a 201-character group name",
        ),
    ];

    for (rejected_insert_sql, rule_description) in rejected_inserts {
        let rejected_insert = sqlx::query(rejected_insert_sql)
            .bind(user_id)
            .execute(&db_pool)
            .await;
        assert!(rejected_insert.is_err(), "should refuse {rule_description}");
    }

    let rejected_task_inserts = [
        (
            "INSERT INTO tasks (id, user_id, group_id, position, name) VALUES (gen_random_uuid(), $1, $2, 0, repeat('a', 501))",
            "a 501-character task name",
        ),
        (
            "INSERT INTO tasks (id, user_id, group_id, position, name) VALUES (gen_random_uuid(), $1, $2, -1, 'Negative')",
            "a negative position",
        ),
        (
            "INSERT INTO tasks (id, user_id, group_id, position, name, estimate_minutes) VALUES (gen_random_uuid(), $1, $2, 0, 'Long', 1441)",
            "an estimate over 24 hours",
        ),
        (
            "INSERT INTO tasks (id, user_id, group_id, position, name, estimate_minutes) VALUES (gen_random_uuid(), $1, $2, 0, 'Zero', 0)",
            "a zero estimate",
        ),
    ];

    for (rejected_insert_sql, rule_description) in rejected_task_inserts {
        let rejected_insert = sqlx::query(rejected_insert_sql)
            .bind(user_id)
            .bind(task_group_id)
            .execute(&db_pool)
            .await;
        assert!(rejected_insert.is_err(), "should refuse {rule_description}");
    }
}

#[sqlx::test(migrations = false)]
async fn gives_every_existing_account_the_four_default_categories_once(db_pool: PgPool) {
    let migrator = Migrator::new(std::path::Path::new("./migrations"))
        .await
        .expect("migrations should load");
    let tasks_schema_version = 20261028090000;
    for migration in migrator
        .iter()
        .filter(|migration| migration.version < tasks_schema_version)
    {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&db_pool)
            .await
            .expect("earlier migration should apply");
    }
    let user_id = seed_user(&db_pool, "existing@example.com").await;
    for migration in migrator
        .iter()
        .filter(|migration| migration.version >= tasks_schema_version)
    {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&db_pool)
            .await
            .expect("tasks migration should apply");
    }

    sqlx::raw_sql(DEFAULT_CATEGORY_BACKFILL_SQL)
        .execute(&db_pool)
        .await
        .expect("running the backfill again should succeed");

    let default_categories: Vec<(String, String)> =
        sqlx::query_as("SELECT name::text, color FROM categories WHERE user_id = $1 ORDER BY name")
            .bind(user_id)
            .fetch_all(&db_pool)
            .await
            .expect("category list should succeed");
    assert_eq!(
        default_categories,
        vec![
            ("Goals".to_owned(), "#7c95bd".to_owned()),
            ("Personal".to_owned(), "#3a405a".to_owned()),
            ("Studies".to_owned(), "#be7e86".to_owned()),
            ("Work".to_owned(), "#c6afa3".to_owned()),
        ]
    );
}
