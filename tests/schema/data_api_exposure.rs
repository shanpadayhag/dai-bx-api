use sqlx::PgPool;

#[sqlx::test]
async fn enables_row_level_security_on_every_auth_table(db_pool: PgPool) {
    let tables_without_row_security: Vec<String> = sqlx::query_scalar(
        "SELECT relname::text FROM pg_class
         WHERE relnamespace = 'public'::regnamespace AND relkind = 'r'
           AND relname <> '_sqlx_migrations' AND NOT relrowsecurity
         ORDER BY relname",
    )
    .fetch_all(&db_pool)
    .await
    .expect("catalog query should succeed");

    assert!(
        tables_without_row_security.is_empty(),
        "tables without row level security: {tables_without_row_security:?}"
    );
}
