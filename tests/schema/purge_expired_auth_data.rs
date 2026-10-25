use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, random_hash, seed_session, seed_user};

async fn purge(db_pool: &PgPool) {
    sqlx::query("SELECT purge_expired_auth_data()")
        .execute(db_pool)
        .await
        .expect("purge should succeed");
}

async fn seed_security_event(db_pool: &PgPool, age: &str) {
    sqlx::query(
        "INSERT INTO security_events (id, occurred_at, event_type)
         VALUES (gen_random_uuid(), now() - $1::interval, 'sign_in_failed')",
    )
    .bind(age)
    .execute(db_pool)
    .await
    .expect("security event insert should succeed");
}

async fn seed_session_token(db_pool: &PgPool, session_id: Uuid, expires_in: &str) {
    sqlx::query(
        "INSERT INTO session_tokens (id, session_id, kind, token_hash, expires_at)
         VALUES (gen_random_uuid(), $1, 'access', $2, now() + $3::interval)",
    )
    .bind(session_id)
    .bind(random_hash())
    .bind(expires_in)
    .execute(db_pool)
    .await
    .expect("session token insert should succeed");
}

#[sqlx::test]
async fn purges_security_events_older_than_twelve_months_only(db_pool: PgPool) {
    seed_security_event(&db_pool, "13 months").await;
    seed_security_event(&db_pool, "11 months").await;

    purge(&db_pool).await;

    assert_eq!(count_rows(&db_pool, "security_events").await, 1);
}

#[sqlx::test]
async fn purges_sessions_past_their_idle_or_absolute_limit_or_revoked(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "sessions@example.com").await;
    let fresh_session_id = seed_session(&db_pool, user_id, "1 hour").await;
    seed_session(&db_pool, user_id, "8 days").await;
    let revoked_session_id = seed_session(&db_pool, user_id, "1 hour").await;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(revoked_session_id)
        .execute(&db_pool)
        .await
        .expect("revoke should succeed");
    sqlx::query(
        "INSERT INTO sessions (id, user_id, created_at, last_seen_at)
         VALUES (gen_random_uuid(), $1, now() - interval '31 days', now())",
    )
    .bind(user_id)
    .execute(&db_pool)
    .await
    .expect("old busy session insert should succeed");

    purge(&db_pool).await;

    let remaining_session_id: Uuid = sqlx::query_scalar("SELECT id FROM sessions")
        .fetch_one(&db_pool)
        .await
        .expect("exactly one session should remain");
    assert_eq!(remaining_session_id, fresh_session_id);
}

#[sqlx::test]
async fn purges_expired_session_tokens_but_keeps_live_ones(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "tokens@example.com").await;
    let session_id = seed_session(&db_pool, user_id, "1 hour").await;
    seed_session_token(&db_pool, session_id, "-1 minute").await;
    seed_session_token(&db_pool, session_id, "10 minutes").await;

    purge(&db_pool).await;

    assert_eq!(count_rows(&db_pool, "session_tokens").await, 1);
}

#[sqlx::test]
async fn purges_expired_challenges_email_tokens_devices_and_unused_invites(db_pool: PgPool) {
    let user_id = seed_user(&db_pool, "expired@example.com").await;
    for expires_in in ["-1 minute", "5 minutes"] {
        sqlx::query(
            "INSERT INTO auth_challenges (id, user_id, purpose, token_hash, expires_at)
             VALUES (gen_random_uuid(), $1, 'mfa_verify', $2, now() + $3::interval)",
        )
        .bind(user_id)
        .bind(random_hash())
        .bind(expires_in)
        .execute(&db_pool)
        .await
        .expect("challenge insert should succeed");
        sqlx::query(
            "INSERT INTO email_tokens (id, user_id, purpose, token_hash, expires_at)
             VALUES (gen_random_uuid(), $1, 'password_reset', $2, now() + $3::interval)",
        )
        .bind(user_id)
        .bind(random_hash())
        .bind(expires_in)
        .execute(&db_pool)
        .await
        .expect("email token insert should succeed");
        sqlx::query(
            "INSERT INTO remembered_devices (id, user_id, token_hash, expires_at)
             VALUES (gen_random_uuid(), $1, $2, now() + $3::interval)",
        )
        .bind(user_id)
        .bind(random_hash())
        .bind(expires_in)
        .execute(&db_pool)
        .await
        .expect("remembered device insert should succeed");
        sqlx::query(
            "INSERT INTO invites (id, code_hash, expires_at)
             VALUES (gen_random_uuid(), $1, now() + $2::interval)",
        )
        .bind(random_hash())
        .bind(expires_in)
        .execute(&db_pool)
        .await
        .expect("invite insert should succeed");
    }
    sqlx::query(
        "INSERT INTO invites (id, code_hash, expires_at, used_at, used_by_user_id)
         VALUES (gen_random_uuid(), $1, now() - interval '1 day', now() - interval '2 days', $2)",
    )
    .bind(random_hash())
    .bind(user_id)
    .execute(&db_pool)
    .await
    .expect("used invite insert should succeed");

    purge(&db_pool).await;

    assert_eq!(count_rows(&db_pool, "auth_challenges").await, 1);
    assert_eq!(count_rows(&db_pool, "email_tokens").await, 1);
    assert_eq!(count_rows(&db_pool, "remembered_devices").await, 1);
    assert_eq!(count_rows(&db_pool, "invites").await, 2);
}

#[sqlx::test]
async fn purges_throttles_that_have_been_quiet_for_a_day(db_pool: PgPool) {
    for (throttle_key, quiet_for) in [("stale", "2 days"), ("recent", "1 hour")] {
        sqlx::query(
            "INSERT INTO auth_throttles (key, failures, window_started_at, blocked_until)
             VALUES ($1, 3, now() - $2::interval, now() - $2::interval)",
        )
        .bind(throttle_key)
        .bind(quiet_for)
        .execute(&db_pool)
        .await
        .expect("throttle insert should succeed");
    }

    purge(&db_pool).await;

    let remaining_throttle_key: String = sqlx::query_scalar("SELECT key FROM auth_throttles")
        .fetch_one(&db_pool)
        .await
        .expect("exactly one throttle should remain");
    assert_eq!(remaining_throttle_key, "recent");
}
