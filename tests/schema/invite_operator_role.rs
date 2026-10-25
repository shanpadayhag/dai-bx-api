use sqlx::{PgPool, Postgres, Transaction};

use crate::support::random_hash;

async fn begin_as_invite_operator(db_pool: &PgPool) -> Transaction<'static, Postgres> {
    let mut operator_transaction = db_pool.begin().await.expect("transaction should begin");
    sqlx::query("SET LOCAL ROLE invite_operator")
        .execute(&mut *operator_transaction)
        .await
        .expect("the invite_operator role should exist");
    operator_transaction
}

#[sqlx::test]
async fn invite_operator_can_create_and_read_invites(db_pool: PgPool) {
    let mut operator_transaction = begin_as_invite_operator(&db_pool).await;

    sqlx::query(
        "INSERT INTO invites (id, code_hash, expires_at)
         VALUES (gen_random_uuid(), $1, now() + interval '7 days')",
    )
    .bind(random_hash())
    .execute(&mut *operator_transaction)
    .await
    .expect("invite_operator should insert invites");
    let visible_invite_count: i64 = sqlx::query_scalar("SELECT count(*) FROM invites")
        .fetch_one(&mut *operator_transaction)
        .await
        .expect("invite_operator should read invites");

    assert_eq!(visible_invite_count, 1);
}

#[sqlx::test]
async fn invite_operator_cannot_change_or_delete_invites(db_pool: PgPool) {
    let mut operator_transaction = begin_as_invite_operator(&db_pool).await;

    let delete_result = sqlx::query("DELETE FROM invites")
        .execute(&mut *operator_transaction)
        .await;

    assert!(delete_result.is_err());
}

#[sqlx::test]
async fn invite_operator_is_refused_on_every_other_table(db_pool: PgPool) {
    for other_table in [
        "users",
        "email_tokens",
        "auth_challenges",
        "sessions",
        "session_tokens",
        "remembered_devices",
        "recovery_codes",
        "auth_throttles",
        "security_events",
    ] {
        let mut operator_transaction = begin_as_invite_operator(&db_pool).await;

        let read_result = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT 1 FROM {other_table} LIMIT 1"
        )))
        .execute(&mut *operator_transaction)
        .await;

        assert!(
            read_result.is_err(),
            "invite_operator should not read {other_table}"
        );
    }
}
