use std::time::Duration;

use axum::http::StatusCode;
use daibx_api::core::config::smtp_settings::SmtpSettings;
use daibx_api::core::mail_transport::build_mail_transport;
use daibx_api::features::security_log::entities::security_event::SecurityEventType;
use daibx_api::features::sign_in_alerts::actions::send_pending_sign_in_alerts::SendPendingSignInAlerts;
use daibx_api::features::sign_in_alerts::entities::sign_in_alert::{
    SIGN_IN_ALERT_RUN_BUDGET, SignInAlertTally,
};
use sqlx::PgPool;

use crate::support::{
    SignedInPerson, count_emails_after_a_pause, count_pending_sign_in_alerts,
    count_sent_account_emails, next_totp_code, production_sized_pool, read_latest_email,
    recorded_event_types, sign_in_to_code_step, sign_up_with_authenticator, test_app_config,
    verify_code,
};

const SIGN_UP_EMAIL_COUNT: usize = 1;
const CHROME_ON_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_6) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36";

fn reachable_smtp_settings() -> SmtpSettings {
    test_app_config().smtp_settings
}

fn unreachable_smtp_settings() -> SmtpSettings {
    SmtpSettings {
        port: 1,
        ..reachable_smtp_settings()
    }
}

async fn send_due_alerts_within(
    db_pool: &PgPool,
    smtp_settings: &SmtpSettings,
    time_budget: Duration,
) -> SignInAlertTally {
    let app_config = test_app_config();
    let mail_transport = build_mail_transport(smtp_settings);
    let one_connection_pool = production_sized_pool(db_pool);
    SendPendingSignInAlerts::new(
        &one_connection_pool,
        &mail_transport,
        &smtp_settings.mail_from,
        &app_config.app_origin,
    )
    .execute(time_budget)
    .await
    .expect("sending due alerts should run")
}

async fn send_due_alerts(db_pool: &PgPool, smtp_settings: &SmtpSettings) -> SignInAlertTally {
    send_due_alerts_within(db_pool, smtp_settings, SIGN_IN_ALERT_RUN_BUDGET).await
}

async fn person_with_pending_alert(db_pool: &PgPool, use_recovery_code: bool) -> SignedInPerson {
    let signed_in_person = sign_up_with_authenticator(db_pool).await;
    let challenge_cookie = sign_in_to_code_step(db_pool, &signed_in_person.email).await;
    let entered_code = if use_recovery_code {
        signed_in_person.recovery_codes[0].clone()
    } else {
        next_totp_code(&signed_in_person.totp_secret)
    };
    let response = verify_code(db_pool, &challenge_cookie, &entered_code, false).await;
    assert_eq!(response.status(), StatusCode::OK);
    signed_in_person
}

async fn run_sql(db_pool: &PgPool, sql_statement: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql_statement))
        .execute(db_pool)
        .await
        .expect("the test statement should succeed");
}

fn plain_tally(sent_count: u32, retry_count: u32, dropped_count: u32) -> SignInAlertTally {
    SignInAlertTally {
        sent_count,
        retry_count,
        dropped_count,
    }
}

#[sqlx::test]
async fn sends_the_device_the_time_and_the_warning_with_only_app_links(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, false).await;
    run_sql(
        &db_pool,
        "UPDATE users SET calendar_time_zone = 'Asia/Manila'",
    )
    .await;
    sqlx::query(
        "UPDATE pending_sign_in_alerts
         SET user_agent = $1, signed_in_at = '2026-11-18T13:41:00Z'",
    )
    .bind(CHROME_ON_MAC)
    .execute(&db_pool)
    .await
    .expect("updating the alert should succeed");

    let alert_tally = send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    assert_eq!(alert_tally, plain_tally(1, 0, 0));
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
    let alert_email = read_latest_email(&signed_in_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
    let app_origin = test_app_config().app_origin;
    assert_eq!(alert_email.subject, "New sign-in to your DaiBX account");
    for email_body in [&alert_email.text_body, &alert_email.html_body] {
        assert!(email_body.contains("Chrome on macOS"));
        assert!(email_body.contains("Wed, 18 Nov 2026, 9:41 PM (Asia/Manila)"));
        assert!(email_body.contains("never asks for your password or your codes"));
        assert!(email_body.contains(&format!("{app_origin}/forgot-password")));
        assert!(!email_body.contains("180.191.77.129"), "no IP address");
    }
    assert!(
        alert_email
            .text_body
            .split_whitespace()
            .filter(|text_word| text_word.starts_with("http"))
            .all(|text_link| text_link.starts_with(&app_origin))
    );
    assert_eq!(alert_email.html_body.matches("href=").count(), 1);
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SignInAlertSent)
    );
}

#[sqlx::test]
async fn uses_utc_without_a_time_zone_and_names_a_recovery_code(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, true).await;
    run_sql(
        &db_pool,
        "UPDATE pending_sign_in_alerts SET signed_in_at = '2026-11-18T13:41:00Z'",
    )
    .await;

    send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    let alert_email = read_latest_email(&signed_in_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
    assert!(
        alert_email
            .text_body
            .contains("Wed, 18 Nov 2026, 1:41 PM (UTC)")
    );
    assert!(alert_email.text_body.contains("with a recovery code"));
    assert!(alert_email.text_body.contains("Unknown device"));
}

#[sqlx::test]
async fn sends_every_due_alert_in_one_run(db_pool: PgPool) {
    let first_person = person_with_pending_alert(&db_pool, false).await;
    let second_person = person_with_pending_alert(&db_pool, false).await;

    let alert_tally = send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    assert_eq!(alert_tally, plain_tally(2, 0, 0));
    read_latest_email(&first_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
    read_latest_email(&second_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
}

#[sqlx::test]
async fn retries_a_failed_send_a_minute_later(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, false).await;

    let failed_tally = send_due_alerts(&db_pool, &unreachable_smtp_settings()).await;

    assert_eq!(failed_tally, plain_tally(0, 1, 0));
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 1);
    let was_postponed: bool = sqlx::query_scalar(
        "SELECT next_attempt_at BETWEEN now() + interval '50 seconds' AND now() + interval '60 seconds'
         FROM pending_sign_in_alerts",
    )
    .fetch_one(&db_pool)
    .await
    .expect("reading the alert should succeed");
    assert!(was_postponed);
    assert_eq!(
        send_due_alerts(&db_pool, &reachable_smtp_settings()).await,
        SignInAlertTally::default(),
        "the alert isn't due again yet"
    );

    run_sql(
        &db_pool,
        "UPDATE pending_sign_in_alerts SET next_attempt_at = now()",
    )
    .await;

    assert_eq!(
        send_due_alerts(&db_pool, &reachable_smtp_settings()).await,
        plain_tally(1, 0, 0)
    );
    read_latest_email(&signed_in_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
}

#[sqlx::test]
async fn drops_an_alert_that_still_fails_after_an_hour(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, false).await;
    run_sql(
        &db_pool,
        "UPDATE pending_sign_in_alerts SET signed_in_at = now() - interval '61 minutes'",
    )
    .await;

    let alert_tally = send_due_alerts(&db_pool, &unreachable_smtp_settings()).await;

    assert_eq!(alert_tally, plain_tally(0, 0, 1));
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 0);
    assert!(
        recorded_event_types(&db_pool, &signed_in_person.email)
            .await
            .contains(&SecurityEventType::SignInAlertFailed)
    );
}

#[sqlx::test]
async fn sends_alerts_with_the_overall_email_cap_full(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, false).await;
    count_sent_account_emails(&db_pool, 100).await;

    let alert_tally = send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    assert_eq!(alert_tally, plain_tally(1, 0, 0));
    read_latest_email(&signed_in_person.email, SIGN_UP_EMAIL_COUNT + 1).await;
}

#[sqlx::test]
async fn counts_a_sent_alert_toward_the_overall_email_cap(db_pool: PgPool) {
    person_with_pending_alert(&db_pool, false).await;
    let cap_events_sql =
        "SELECT count(*) FROM security_events WHERE event_type = 'email_cap_reached'";
    run_sql(&db_pool, "DELETE FROM auth_throttles").await;
    count_sent_account_emails(&db_pool, 99).await;

    send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    let cap_event_count: i64 = sqlx::query_scalar(cap_events_sql)
        .fetch_one(&db_pool)
        .await
        .expect("counting events should succeed");
    assert_eq!(cap_event_count, 1, "the alert was the hundredth email");
}

#[sqlx::test]
async fn leaves_an_alert_that_another_run_is_sending(db_pool: PgPool) {
    let signed_in_person = person_with_pending_alert(&db_pool, false).await;
    let mut other_run = db_pool.begin().await.expect("a transaction");
    sqlx::query("SELECT id FROM pending_sign_in_alerts FOR UPDATE")
        .fetch_one(&mut *other_run)
        .await
        .expect("locking the alert should succeed");

    let alert_tally = send_due_alerts(&db_pool, &reachable_smtp_settings()).await;

    assert_eq!(alert_tally, SignInAlertTally::default());
    assert_eq!(
        count_emails_after_a_pause(&signed_in_person.email).await,
        SIGN_UP_EMAIL_COUNT
    );
}

#[sqlx::test]
async fn stops_when_the_time_budget_is_spent(db_pool: PgPool) {
    person_with_pending_alert(&db_pool, false).await;

    let alert_tally =
        send_due_alerts_within(&db_pool, &reachable_smtp_settings(), Duration::ZERO).await;

    assert_eq!(alert_tally, SignInAlertTally::default());
    assert_eq!(count_pending_sign_in_alerts(&db_pool).await, 1);
}
