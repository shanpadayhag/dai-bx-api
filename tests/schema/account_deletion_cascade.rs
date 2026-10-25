use sqlx::PgPool;

use crate::support::{count_rows, random_hash, seed_session, seed_user};

const USER_OWNED_TABLES: [&str; 7] = [
    "email_tokens",
    "auth_challenges",
    "sessions",
    "session_tokens",
    "remembered_devices",
    "recovery_codes",
    "users",
];

#[sqlx::test]
async fn deleting_a_user_removes_every_row_they_own(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "owner@example.com").await;
    let session_id = seed_session(&db_pool, user_id, "1 minute").await;
    sqlx::query(
        "INSERT INTO email_tokens (id, user_id, purpose, token_hash, expires_at)
         VALUES (gen_random_uuid(), $1, 'verify_email', $2, now() + interval '1 day')",
    )
    .bind(user_id)
    .bind(random_hash())
    .execute(&db_pool)
    .await
    .expect("email token insert should succeed");
    sqlx::query(
        "INSERT INTO auth_challenges (id, user_id, purpose, token_hash, expires_at)
         VALUES (gen_random_uuid(), $1, 'mfa_verify', $2, now() + interval '5 minutes')",
    )
    .bind(user_id)
    .bind(random_hash())
    .execute(&db_pool)
    .await
    .expect("challenge insert should succeed");
    sqlx::query(
        "INSERT INTO session_tokens (id, session_id, kind, token_hash, expires_at)
         VALUES (gen_random_uuid(), $1, 'refresh', $2, now() + interval '7 days')",
    )
    .bind(session_id)
    .bind(random_hash())
    .execute(&db_pool)
    .await
    .expect("session token insert should succeed");
    sqlx::query(
        "INSERT INTO remembered_devices (id, user_id, token_hash, expires_at)
         VALUES (gen_random_uuid(), $1, $2, now() + interval '30 days')",
    )
    .bind(user_id)
    .bind(random_hash())
    .execute(&db_pool)
    .await
    .expect("remembered device insert should succeed");
    sqlx::query(
        "INSERT INTO recovery_codes (id, user_id, salt, code_hash)
         VALUES (gen_random_uuid(), $1, $2, $3)",
    )
    .bind(user_id)
    .bind(random_hash())
    .bind(random_hash())
    .execute(&db_pool)
    .await
    .expect("recovery code insert should succeed");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    for user_owned_table in USER_OWNED_TABLES {
        assert_eq!(
            count_rows(&db_pool, user_owned_table).await,
            0,
            "{user_owned_table} should be empty after the user is deleted"
        );
    }
}

#[sqlx::test]
async fn deleting_a_user_keeps_the_invite_they_used_with_no_owner(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "invited@example.com").await;
    sqlx::query(
        "INSERT INTO invites (id, code_hash, expires_at, used_at, used_by_user_id)
         VALUES (gen_random_uuid(), $1, now() + interval '7 days', now(), $2)",
    )
    .bind(random_hash())
    .bind(user_id)
    .execute(&db_pool)
    .await
    .expect("invite insert should succeed");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    let remaining_owner_count: i64 =
        sqlx::query_scalar("SELECT count(used_by_user_id) FROM invites")
            .fetch_one(&db_pool)
            .await
            .expect("invite count should succeed");
    assert_eq!(count_rows(&db_pool, "invites").await, 1);
    assert_eq!(remaining_owner_count, 0);
}

#[sqlx::test]
async fn refuses_two_accounts_with_the_same_email_in_any_case(db_pool: PgPool) {
    seed_user(&db_pool, "Person@Example.com").await;

    let duplicate_insert = sqlx::query(
        "INSERT INTO users (id, security_ref, name, email, password_hash, terms_version, terms_presented_at)
         VALUES (gen_random_uuid(), gen_random_uuid(), 'Copy', 'person@example.com', 'x', 'draft-1', now())",
    )
    .execute(&db_pool)
    .await;

    assert!(duplicate_insert.is_err());
}
