use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::support::random_hash;

async fn begin_as_app_role(db_pool: &PgPool) -> Transaction<'static, Postgres> {
    let mut app_transaction = db_pool.begin().await.expect("transaction should begin");
    sqlx::query("SET LOCAL ROLE daibx_app")
        .execute(&mut *app_transaction)
        .await
        .expect("the daibx_app role should exist");
    app_transaction
}

async fn app_role_is_refused(db_pool: &PgPool, refused_statement: &str) -> bool {
    let mut app_transaction = begin_as_app_role(db_pool).await;
    let statement_result = sqlx::query(sqlx::AssertSqlSafe(refused_statement.to_owned()))
        .execute(&mut *app_transaction)
        .await;
    match statement_result {
        Err(sqlx::Error::Database(database_error)) => {
            database_error.code().as_deref() == Some("42501")
        }
        _ => false,
    }
}

async fn insert_security_event(db_executor: impl sqlx::PgExecutor<'_>) {
    sqlx::query("INSERT INTO security_events (id, event_type) VALUES ($1, 'sign_in_succeeded')")
        .bind(Uuid::now_v7())
        .execute(db_executor)
        .await
        .expect("the security event should be added");
}

#[sqlx::test]
async fn the_app_role_reads_and_writes_app_tables(db_pool: PgPool) {
    let mut app_transaction = begin_as_app_role(&db_pool).await;
    let invite_id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO invites (id, code_hash, expires_at)
         VALUES ($1, $2, now() + interval '7 days')",
    )
    .bind(invite_id)
    .bind(random_hash())
    .execute(&mut *app_transaction)
    .await
    .expect("the app role should add a row");
    let changed_row = sqlx::query("UPDATE invites SET expires_at = now() WHERE id = $1")
        .bind(invite_id)
        .execute(&mut *app_transaction)
        .await
        .expect("the app role should change a row");
    let deleted_row = sqlx::query("DELETE FROM invites WHERE id = $1")
        .bind(invite_id)
        .execute(&mut *app_transaction)
        .await
        .expect("the app role should delete a row");

    assert_eq!(changed_row.rows_affected(), 1);
    assert_eq!(deleted_row.rows_affected(), 1);
}

#[sqlx::test]
async fn every_app_table_is_open_to_the_app_role(db_pool: PgPool) {
    let closed_tables: Vec<String> = sqlx::query_scalar(
        "SELECT app_table.tablename::text
         FROM pg_tables AS app_table
         WHERE app_table.schemaname = 'public'
           AND app_table.tablename NOT IN ('_sqlx_migrations', 'security_events')
           AND NOT (
               has_table_privilege('daibx_app', quote_ident(app_table.tablename), 'SELECT')
               AND has_table_privilege('daibx_app', quote_ident(app_table.tablename), 'INSERT')
               AND has_table_privilege('daibx_app', quote_ident(app_table.tablename), 'UPDATE')
               AND has_table_privilege('daibx_app', quote_ident(app_table.tablename), 'DELETE')
               AND EXISTS (
                   SELECT FROM pg_policies AS app_policy
                   WHERE app_policy.schemaname = 'public'
                     AND app_policy.tablename = app_table.tablename
                     AND app_policy.cmd = 'ALL'
                     AND 'daibx_app' = ANY (app_policy.roles)
               )
           )
         ORDER BY 1",
    )
    .fetch_all(&db_pool)
    .await
    .expect("catalog query should succeed");

    assert!(
        closed_tables.is_empty(),
        "each new table needs a daibx_app policy, missing on: {closed_tables:?}"
    );
}

#[sqlx::test]
async fn the_app_role_adds_and_reads_security_events(db_pool: PgPool) {
    let mut app_transaction = begin_as_app_role(&db_pool).await;

    insert_security_event(&mut *app_transaction).await;
    let visible_event_count: i64 = sqlx::query_scalar("SELECT count(*) FROM security_events")
        .fetch_one(&mut *app_transaction)
        .await
        .expect("the app role should read security events");

    assert_eq!(visible_event_count, 1);
}

#[sqlx::test]
async fn the_app_role_cannot_change_or_remove_security_events(db_pool: PgPool) {
    insert_security_event(&db_pool).await;

    for refused_statement in [
        "UPDATE security_events SET details = '{}'::jsonb",
        "DELETE FROM security_events",
        "TRUNCATE security_events",
    ] {
        assert!(
            app_role_is_refused(&db_pool, refused_statement).await,
            "the app role should be refused: {refused_statement}"
        );
    }
}

#[sqlx::test]
async fn the_app_role_cannot_change_the_schema(db_pool: PgPool) {
    for refused_statement in [
        "CREATE TABLE public.not_allowed (id integer)",
        "ALTER TABLE users ADD COLUMN not_allowed integer",
        "DROP TABLE users",
        "TRUNCATE users CASCADE",
        "CREATE ROLE not_allowed",
    ] {
        assert!(
            app_role_is_refused(&db_pool, refused_statement).await,
            "the app role should be refused: {refused_statement}"
        );
    }
}

#[sqlx::test]
async fn the_app_role_cannot_reach_migrations_or_the_purge(db_pool: PgPool) {
    for refused_statement in [
        "SELECT 1 FROM _sqlx_migrations LIMIT 1",
        "SELECT purge_expired_auth_data()",
    ] {
        assert!(
            app_role_is_refused(&db_pool, refused_statement).await,
            "the app role should be refused: {refused_statement}"
        );
    }
}

#[sqlx::test]
async fn the_app_role_has_no_special_powers(db_pool: PgPool) {
    let has_special_power: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolcreaterole OR rolcreatedb OR rolreplication OR rolbypassrls
         FROM pg_roles WHERE rolname = 'daibx_app'",
    )
    .fetch_one(&db_pool)
    .await
    .expect("the daibx_app role should exist");

    assert!(!has_special_power);
}

#[sqlx::test]
async fn a_table_added_later_is_granted_to_the_app_role(db_pool: PgPool) {
    sqlx::query("CREATE TABLE added_later (id uuid PRIMARY KEY)")
        .execute(&db_pool)
        .await
        .expect("the owner should create a table");

    let can_write: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('daibx_app', 'added_later', 'SELECT')
            AND has_table_privilege('daibx_app', 'added_later', 'INSERT')
            AND has_table_privilege('daibx_app', 'added_later', 'UPDATE')
            AND has_table_privilege('daibx_app', 'added_later', 'DELETE')",
    )
    .fetch_one(&db_pool)
    .await
    .expect("catalog query should succeed");

    assert!(can_write);
}
