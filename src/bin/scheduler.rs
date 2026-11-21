use std::env;
use std::time::Duration;

use daibx_api::core::config::app_config::{AppConfigError, read_app_origin};
use daibx_api::core::config::smtp_settings::SmtpSettings;
use daibx_api::core::config::vapid_settings::VapidSettings;
use daibx_api::core::database::database_connect_options;
use daibx_api::core::http_client::build_http_client;
use daibx_api::core::logging::init_logging;
use daibx_api::core::mail_transport::{MailTransport, build_mail_transport};
use daibx_api::core::utc_instant::UtcInstant;
use daibx_api::features::card_schedules::actions::run_due_schedules::RunDueSchedules;
use daibx_api::features::cards::actions::archive_idle_cards::ArchiveIdleCards;
use daibx_api::features::notifications::actions::send_due_notifications::{
    PUSH_SEND_BUDGET, PushTally, SendDueNotifications,
};
use daibx_api::features::sign_in_alerts::actions::send_pending_sign_in_alerts::SendPendingSignInAlerts;
use daibx_api::features::sign_in_alerts::entities::sign_in_alert::{
    SIGN_IN_ALERT_RUN_BUDGET, SignInAlertTally,
};
use lambda_http::lambda_runtime::{self, LambdaEvent, service_fn};
use lettre::message::Mailbox;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

const DATABASE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

struct SignInAlertMail {
    mail_transport: MailTransport,
    mail_from: Mailbox,
    app_origin: String,
}

fn read_sign_in_alert_mail() -> Result<SignInAlertMail, AppConfigError> {
    let smtp_settings = SmtpSettings::from_variables(|variable_name| env::var(variable_name).ok())?;
    Ok(SignInAlertMail {
        mail_transport: build_mail_transport(&smtp_settings),
        mail_from: smtp_settings.mail_from,
        app_origin: read_app_origin()?,
    })
}

async fn send_sign_in_alerts(
    db_pool: &PgPool,
    sign_in_alert_mail: Option<&SignInAlertMail>,
) -> SignInAlertTally {
    let Some(sign_in_alert_mail) = sign_in_alert_mail else {
        return SignInAlertTally::default();
    };
    SendPendingSignInAlerts::new(
        db_pool,
        &sign_in_alert_mail.mail_transport,
        &sign_in_alert_mail.mail_from,
        &sign_in_alert_mail.app_origin,
    )
    .execute(SIGN_IN_ALERT_RUN_BUDGET)
    .await
    .unwrap_or_else(|alert_error| {
        tracing::error!(error = %alert_error, "sign-in alerts could not be sent");
        SignInAlertTally::default()
    })
}

struct PushSending {
    http_client: Client,
    vapid_settings: VapidSettings,
}

async fn send_due_notifications(
    db_pool: &PgPool,
    push_sending: Option<&PushSending>,
    now: UtcInstant,
) -> PushTally {
    let Some(push_sending) = push_sending else {
        return PushTally::default();
    };
    SendDueNotifications::new(
        db_pool,
        &push_sending.http_client,
        &push_sending.vapid_settings,
    )
    .execute(now, PUSH_SEND_BUDGET)
    .await
    .unwrap_or_else(|push_error| {
        tracing::error!(error = %push_error, "due notifications could not be sent");
        PushTally::default()
    })
}

async fn run_minute_jobs(
    db_pool: &PgPool,
    sign_in_alert_mail: Option<&SignInAlertMail>,
    push_sending: Option<&PushSending>,
) -> Result<Value, lambda_runtime::Error> {
    let now = UtcInstant::now();
    let due_run_tally = RunDueSchedules::new(db_pool).execute(now).await?;
    let idle_archive_tally = ArchiveIdleCards::new(db_pool).execute(now).await?;
    let sign_in_alert_tally = send_sign_in_alerts(db_pool, sign_in_alert_mail).await;
    let push_tally = send_due_notifications(db_pool, push_sending, now).await;
    tracing::info!(
        ran_count = due_run_tally.ran_count,
        failed_count = due_run_tally.failed_count,
        archived_count = idle_archive_tally.archived_count,
        alerts_sent_count = sign_in_alert_tally.sent_count,
        alerts_retry_count = sign_in_alert_tally.retry_count,
        alerts_dropped_count = sign_in_alert_tally.dropped_count,
        pushes_sent_count = push_tally.sent_count,
        pushes_held_back_count = push_tally.held_back_count,
        pushes_gone_count = push_tally.gone_count,
        pushes_failed_count = push_tally.failed_count,
        deliveries_pruned_count = push_tally.pruned_count,
        "due schedules ran, idle cards archived, sign-in alerts and notifications handled"
    );
    Ok(json!({
        "ranCount": due_run_tally.ran_count,
        "failedCount": due_run_tally.failed_count,
        "archivedCount": idle_archive_tally.archived_count,
        "signInAlertsSent": sign_in_alert_tally.sent_count,
        "notificationsSent": push_tally.sent_count,
    }))
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    init_logging();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connect_options =
        database_connect_options(&database_url).expect("DATABASE_URL must be a valid address");
    let db_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(DATABASE_ACQUIRE_TIMEOUT)
        .connect_with(connect_options)
        .await
        .expect("the database must be reachable at startup");
    let db_pool = &db_pool;
    let sign_in_alert_mail = read_sign_in_alert_mail()
        .inspect_err(|config_error| {
            tracing::error!(error = %config_error, "sign-in alerts are off until the mail settings are set");
        })
        .ok();
    let sign_in_alert_mail = sign_in_alert_mail.as_ref();
    let push_sending = VapidSettings::from_variables(|variable_name| env::var(variable_name).ok())
        .map(|vapid_settings| PushSending {
            http_client: build_http_client(),
            vapid_settings,
        })
        .inspect_err(|vapid_error| {
            tracing::error!(error = %vapid_error, "notifications are off until the VAPID settings are set");
        })
        .ok();
    let push_sending = push_sending.as_ref();
    lambda_runtime::run(service_fn(|_: LambdaEvent<Value>| {
        run_minute_jobs(db_pool, sign_in_alert_mail, push_sending)
    }))
    .await
}
