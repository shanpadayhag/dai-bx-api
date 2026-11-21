use sqlx::types::Uuid;
use sqlx::{PgPool, Postgres, Transaction};

use crate::support::{count_rows, seed_user};

struct SeededOwner {
    user_id: Uuid,
    task_id: Uuid,
    card_id: Uuid,
}

async fn seed_owner(db_pool: &PgPool, email: &str) -> SeededOwner {
    let user_id = seed_user(db_pool, email).await;
    let task_id: Uuid = sqlx::query_scalar(
        "WITH new_group AS (
             INSERT INTO task_groups (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Work')
             RETURNING id
         )
         INSERT INTO tasks (id, user_id, group_id, position, name)
         SELECT gen_random_uuid(), $1, id, 0, 'Write the plan' FROM new_group
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed task insert should succeed");
    let card_id: Uuid = sqlx::query_scalar(
        "WITH new_board AS (
             INSERT INTO boards (id, user_id, name) VALUES (gen_random_uuid(), $1, 'Projects')
             RETURNING id
         ), new_list AS (
             INSERT INTO board_lists (id, user_id, board_id, position, name)
             SELECT gen_random_uuid(), $1, id, 0, 'Doing' FROM new_board
             RETURNING id, board_id
         )
         INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
         SELECT gen_random_uuid(), $1, board_id, id, 0, 'Write the intro', '' FROM new_list
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(db_pool)
    .await
    .expect("seed card insert should succeed");
    SeededOwner {
        user_id,
        task_id,
        card_id,
    }
}

async fn insert_entry(
    db_executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    kind: &str,
    task_id: Option<Uuid>,
    card_id: Option<Uuid>,
    title: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO activity_entries (id, user_id, occurred_at, kind, task_id, card_id, title, context_name)
         VALUES (gen_random_uuid(), $1, now(), $2, $3, $4, $5, 'Work')",
    )
    .bind(user_id)
    .bind(kind)
    .bind(task_id)
    .bind(card_id)
    .bind(title)
    .execute(db_executor)
    .await
    .map(|_| ())
}

async fn seed_task_and_card_entries(db_pool: &PgPool, owner: &SeededOwner) {
    insert_entry(
        db_pool,
        owner.user_id,
        "started",
        Some(owner.task_id),
        None,
        Some("Write the plan"),
    )
    .await
    .expect("task entry insert should succeed");
    insert_entry(
        db_pool,
        owner.user_id,
        "card_moved",
        None,
        Some(owner.card_id),
        Some("Write the intro"),
    )
    .await
    .expect("card entry insert should succeed");
}

fn is_check_violation(statement_result: &Result<(), sqlx::Error>) -> bool {
    matches!(
        statement_result,
        Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some("23514")
    )
}

async fn begin_as_app_role(db_pool: &PgPool) -> Transaction<'static, Postgres> {
    let mut app_transaction = db_pool.begin().await.expect("transaction should begin");
    sqlx::query("SET LOCAL ROLE daibx_app")
        .execute(&mut *app_transaction)
        .await
        .expect("the daibx_app role should exist");
    app_transaction
}

#[sqlx::test]
async fn deleting_a_user_removes_only_their_activity(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;
    seed_task_and_card_entries(&db_pool, &owner).await;
    seed_task_and_card_entries(&db_pool, &other_owner).await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner.user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    assert_eq!(count_rows(&db_pool, "activity_entries").await, 2);
}

#[sqlx::test]
async fn deleting_a_task_or_card_keeps_its_entries_without_the_link(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    seed_task_and_card_entries(&db_pool, &owner).await;
    let mut app_transaction = begin_as_app_role(&db_pool).await;

    sqlx::query("DELETE FROM tasks WHERE id = $1")
        .bind(owner.task_id)
        .execute(&mut *app_transaction)
        .await
        .expect("the app role should delete the task");
    sqlx::query("DELETE FROM cards WHERE id = $1")
        .bind(owner.card_id)
        .execute(&mut *app_transaction)
        .await
        .expect("the app role should delete the card");
    app_transaction
        .commit()
        .await
        .expect("the deletes should commit");

    let remaining_links: Vec<(Option<Uuid>, Option<Uuid>, String)> =
        sqlx::query_as("SELECT task_id, card_id, title FROM activity_entries ORDER BY title")
            .fetch_all(&db_pool)
            .await
            .expect("entries should be readable");
    assert_eq!(
        remaining_links,
        vec![
            (None, None, String::from("Write the intro")),
            (None, None, String::from("Write the plan")),
        ]
    );
}

#[sqlx::test]
async fn refuses_an_entry_linked_to_both_a_task_and_a_card(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let insert_result = insert_entry(
        &db_pool,
        owner.user_id,
        "started",
        Some(owner.task_id),
        Some(owner.card_id),
        Some("Write the plan"),
    )
    .await;

    assert!(is_check_violation(&insert_result));
}

#[sqlx::test]
async fn refuses_an_unknown_kind_and_a_missing_title(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;

    let unknown_kind_result = insert_entry(
        &db_pool,
        owner.user_id,
        "renamed",
        None,
        None,
        Some("Write the plan"),
    )
    .await;
    let missing_title_result =
        insert_entry(&db_pool, owner.user_id, "started", None, None, None).await;
    let plan_without_title_result =
        insert_entry(&db_pool, owner.user_id, "plan_accepted", None, None, None).await;

    assert!(is_check_violation(&unknown_kind_result));
    assert!(is_check_violation(&missing_title_result));
    assert!(plan_without_title_result.is_ok());
}

#[sqlx::test]
async fn refuses_a_link_to_another_users_task(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let other_owner = seed_owner(&db_pool, "other@example.com").await;

    let insert_result = insert_entry(
        &db_pool,
        owner.user_id,
        "started",
        Some(other_owner.task_id),
        None,
        Some("Write the plan"),
    )
    .await;

    assert!(matches!(
        insert_result,
        Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some("23503")
    ));
}

#[sqlx::test]
async fn the_app_role_adds_and_reads_activity_but_cannot_change_it(db_pool: PgPool) {
    let owner = seed_owner(&db_pool, "owner@example.com").await;
    let mut app_transaction = begin_as_app_role(&db_pool).await;
    insert_entry(
        &mut *app_transaction,
        owner.user_id,
        "started",
        Some(owner.task_id),
        None,
        Some("Write the plan"),
    )
    .await
    .expect("the app role should add an entry");
    let visible_entry_count: i64 = sqlx::query_scalar("SELECT count(*) FROM activity_entries")
        .fetch_one(&mut *app_transaction)
        .await
        .expect("the app role should read entries");
    app_transaction
        .commit()
        .await
        .expect("the insert should commit");
    assert_eq!(visible_entry_count, 1);

    for refused_statement in [
        "UPDATE activity_entries SET title = 'Changed'",
        "DELETE FROM activity_entries",
        "TRUNCATE activity_entries",
    ] {
        let mut app_transaction = begin_as_app_role(&db_pool).await;
        let statement_result = sqlx::query(sqlx::AssertSqlSafe(refused_statement.to_owned()))
            .execute(&mut *app_transaction)
            .await;
        assert!(
            matches!(
                statement_result,
                Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some("42501")
            ),
            "the app role should be refused: {refused_statement}"
        );
    }
}
