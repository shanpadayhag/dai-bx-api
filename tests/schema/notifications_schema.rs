use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::support::{count_rows, seed_session, seed_user};

const SAMPLE_P256DH: &str =
    "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
const SAMPLE_AUTH_SECRET: &str = "tBHItJI5svbpez7KI4CCXg";

async fn insert_device(
    db_executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    session_id: Option<Uuid>,
    endpoint: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO push_devices (id, user_id, session_id, endpoint, p256dh, auth_secret, device_name, time_zone)
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'iPhone · Safari', 'Asia/Manila')
         RETURNING id",
    )
    .bind(user_id)
    .bind(session_id)
    .bind(endpoint)
    .bind(SAMPLE_P256DH)
    .bind(SAMPLE_AUTH_SECRET)
    .fetch_one(db_executor)
    .await
}

async fn insert_delivery(
    db_executor: impl sqlx::PgExecutor<'_>,
    device_id: Uuid,
    user_id: Uuid,
    kind: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO push_deliveries (device_id, user_id, kind, subject_key, due_at)
         VALUES ($1, $2, $3, 'subject', '2026-11-22T08:00:00Z')",
    )
    .bind(device_id)
    .bind(user_id)
    .bind(kind)
    .execute(db_executor)
    .await
    .map(|_| ())
}

fn has_error_code(statement_result: &Result<impl Sized, sqlx::Error>, error_code: &str) -> bool {
    matches!(
        statement_result,
        Err(sqlx::Error::Database(database_error)) if database_error.code().as_deref() == Some(error_code)
    )
}

#[sqlx::test]
async fn deleting_a_user_removes_only_their_devices_and_deliveries(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_owner_id = seed_user(&db_pool, "other@example.com").await;
    let owner_device_id = insert_device(&db_pool, owner_id, None, "https://fcm.googleapis.com/a")
        .await
        .expect("device insert should succeed");
    let other_device_id = insert_device(
        &db_pool,
        other_owner_id,
        None,
        "https://fcm.googleapis.com/b",
    )
    .await
    .expect("device insert should succeed");
    insert_delivery(&db_pool, owner_device_id, owner_id, "check_in")
        .await
        .expect("delivery insert should succeed");
    insert_delivery(&db_pool, other_device_id, other_owner_id, "check_in")
        .await
        .expect("delivery insert should succeed");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner_id)
        .execute(&db_pool)
        .await
        .expect("user delete should succeed");

    assert_eq!(count_rows(&db_pool, "push_devices").await, 1);
    assert_eq!(count_rows(&db_pool, "push_deliveries").await, 1);
}

#[sqlx::test]
async fn purging_a_session_keeps_its_device_without_the_link(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let session_id = seed_session(&db_pool, owner_id, "40 days").await;
    insert_device(
        &db_pool,
        owner_id,
        Some(session_id),
        "https://fcm.googleapis.com/a",
    )
    .await
    .expect("device insert should succeed");

    sqlx::query("SELECT purge_expired_auth_data()")
        .execute(&db_pool)
        .await
        .expect("purge should succeed");

    let remaining_session_ids: Vec<Option<Uuid>> =
        sqlx::query_scalar("SELECT session_id FROM push_devices")
            .fetch_all(&db_pool)
            .await
            .expect("devices should be readable");
    assert_eq!(remaining_session_ids, vec![None]);
}

#[sqlx::test]
async fn deleting_a_device_removes_its_deliveries(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let device_id = insert_device(&db_pool, owner_id, None, "https://fcm.googleapis.com/a")
        .await
        .expect("device insert should succeed");
    insert_delivery(&db_pool, device_id, owner_id, "block_starting")
        .await
        .expect("delivery insert should succeed");

    sqlx::query("DELETE FROM push_devices WHERE id = $1")
        .bind(device_id)
        .execute(&db_pool)
        .await
        .expect("device delete should succeed");

    assert_eq!(count_rows(&db_pool, "push_deliveries").await, 0);
}

#[sqlx::test]
async fn refuses_a_second_device_with_the_same_endpoint(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_owner_id = seed_user(&db_pool, "other@example.com").await;
    insert_device(&db_pool, owner_id, None, "https://fcm.googleapis.com/a")
        .await
        .expect("device insert should succeed");

    let duplicate_result = insert_device(
        &db_pool,
        other_owner_id,
        None,
        "https://fcm.googleapis.com/a",
    )
    .await;

    assert!(has_error_code(&duplicate_result, "23505"));
}

#[sqlx::test]
async fn refuses_an_unknown_kind_and_a_second_claim_of_one_delivery(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let device_id = insert_device(&db_pool, owner_id, None, "https://fcm.googleapis.com/a")
        .await
        .expect("device insert should succeed");
    insert_delivery(&db_pool, device_id, owner_id, "schedule_ran")
        .await
        .expect("delivery insert should succeed");

    let unknown_kind_result = insert_delivery(&db_pool, device_id, owner_id, "due_date").await;
    let second_claim_result = insert_delivery(&db_pool, device_id, owner_id, "schedule_ran").await;

    assert!(has_error_code(&unknown_kind_result, "23514"));
    assert!(has_error_code(&second_claim_result, "23505"));
}

#[sqlx::test]
async fn refuses_a_delivery_for_another_users_device(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let other_owner_id = seed_user(&db_pool, "other@example.com").await;
    let device_id = insert_device(&db_pool, owner_id, None, "https://fcm.googleapis.com/a")
        .await
        .expect("device insert should succeed");

    let foreign_delivery_result =
        insert_delivery(&db_pool, device_id, other_owner_id, "check_in").await;

    assert!(has_error_code(&foreign_delivery_result, "23503"));
}

#[sqlx::test]
async fn new_users_have_every_notification_switch_on(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;

    let notification_switches: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT notify_check_ins, notify_schedule_runs, notify_blocks_starting, notify_quiet_outside_working_hours
         FROM users WHERE id = $1",
    )
    .bind(owner_id)
    .fetch_one(&db_pool)
    .await
    .expect("switches should be readable");

    assert_eq!(notification_switches, (true, true, true, true));
}

#[sqlx::test]
async fn the_security_log_accepts_push_device_events(db_pool: PgPool) {
    for push_event_type in ["push_device_added", "push_device_removed"] {
        sqlx::query(
            "INSERT INTO security_events (id, event_type) VALUES (gen_random_uuid(), $1::security_event_type)",
        )
        .bind(push_event_type)
        .execute(&db_pool)
        .await
        .expect("push device events should be accepted");
    }

    assert_eq!(count_rows(&db_pool, "security_events").await, 2);
}

#[sqlx::test]
async fn revoking_a_session_removes_only_its_devices_and_logs_each(db_pool: PgPool) {
    let owner_id = seed_user(&db_pool, "owner@example.com").await;
    let revoked_session_id = seed_session(&db_pool, owner_id, "1 hour").await;
    let kept_session_id = seed_session(&db_pool, owner_id, "1 hour").await;
    insert_device(
        &db_pool,
        owner_id,
        Some(revoked_session_id),
        "https://fcm.googleapis.com/a",
    )
    .await
    .expect("device insert should succeed");
    insert_device(
        &db_pool,
        owner_id,
        Some(kept_session_id),
        "https://fcm.googleapis.com/b",
    )
    .await
    .expect("device insert should succeed");

    for _ in 0..2 {
        sqlx::query(
            "UPDATE sessions SET revoked_at = COALESCE(revoked_at, now()), revoked_reason = 'signed_out' WHERE id = $1",
        )
        .bind(revoked_session_id)
        .execute(&db_pool)
        .await
        .expect("revoke should succeed");
    }

    let remaining_session_ids: Vec<Option<Uuid>> =
        sqlx::query_scalar("SELECT session_id FROM push_devices")
            .fetch_all(&db_pool)
            .await
            .expect("devices should be readable");
    let removal_reasons: Vec<String> = sqlx::query_scalar(
        "SELECT details->>'reason' FROM security_events WHERE event_type = 'push_device_removed'",
    )
    .fetch_all(&db_pool)
    .await
    .expect("events should be readable");
    assert_eq!(remaining_session_ids, vec![Some(kept_session_id)]);
    assert_eq!(removal_reasons, vec![String::from("session_ended")]);
}
