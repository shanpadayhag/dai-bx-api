use std::time::{Duration, Instant};

use lettre::message::Mailbox;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::core::mail_transport::MailTransport;
use crate::features::email::actions::send_sign_in_alert_email::SendSignInAlertEmail;
use crate::features::email::entities::sign_in_alert_email::SignInAlertEmail;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sign_in_alerts::data_sources::postgres::sign_in_alert_source::PostgresSignInAlertSource;
use crate::features::sign_in_alerts::entities::sign_in_alert::{
    DueSignInAlert, RECOVERY_CODE_SIGN_IN_METHOD, SIGN_IN_ALERT_RETRY_AFTER_SECONDS,
    SIGN_IN_ALERT_RETRY_WINDOW_SECONDS, SignInAlertTally, describe_sign_in_time,
};
use crate::features::sign_in_alerts::entities::sign_in_device::describe_user_agent;
use crate::features::sign_in_alerts::error::SignInAlertError;
use crate::features::throttling::actions::count_account_email::CountAccountEmail;

pub struct SendPendingSignInAlerts<'a> {
    db_pool: &'a PgPool,
    postgres_sign_in_alert_source: PostgresSignInAlertSource,
    send_sign_in_alert_email: SendSignInAlertEmail<'a>,
    count_account_email: CountAccountEmail,
    record_security_event: RecordSecurityEvent,
}

impl<'a> SendPendingSignInAlerts<'a> {
    pub fn new(
        db_pool: &'a PgPool,
        mail_transport: &'a MailTransport,
        mail_from: &'a Mailbox,
        app_origin: &'a str,
    ) -> Self {
        Self {
            db_pool,
            postgres_sign_in_alert_source: PostgresSignInAlertSource,
            send_sign_in_alert_email: SendSignInAlertEmail::new(
                mail_transport,
                mail_from,
                app_origin,
            ),
            count_account_email: CountAccountEmail::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        time_budget: Duration,
    ) -> Result<SignInAlertTally, SignInAlertError> {
        let started_at = Instant::now();
        let mut alert_tally = SignInAlertTally::default();
        while started_at.elapsed() < time_budget {
            let mut alert_transaction = self.db_pool.begin().await?;
            let Some(due_alert) = self
                .postgres_sign_in_alert_source
                .claim_next_due(&mut *alert_transaction, SIGN_IN_ALERT_RETRY_WINDOW_SECONDS)
                .await?
            else {
                break;
            };
            self.deliver(&mut alert_transaction, &due_alert, &mut alert_tally)
                .await?;
            alert_transaction.commit().await?;
        }
        Ok(alert_tally)
    }

    async fn deliver(
        &self,
        connection: &mut PgConnection,
        due_alert: &DueSignInAlert,
        alert_tally: &mut SignInAlertTally,
    ) -> Result<(), SignInAlertError> {
        let device_label = describe_user_agent(due_alert.user_agent.as_deref());
        let signed_in_at_label = describe_sign_in_time(
            due_alert.signed_in_unix_millis,
            due_alert.calendar_time_zone.as_deref(),
        );
        let send_result = self
            .send_sign_in_alert_email
            .execute(&SignInAlertEmail {
                recipient: &due_alert.recipient,
                device_label: &device_label,
                signed_in_at_label: &signed_in_at_label,
                used_recovery_code: due_alert.sign_in_method == RECOVERY_CODE_SIGN_IN_METHOD,
            })
            .await;
        if send_result.is_ok() {
            self.postgres_sign_in_alert_source
                .delete(&mut *connection, due_alert.id)
                .await?;
            self.count_account_email.execute(&mut *connection).await?;
            self.record(
                connection,
                SecurityEventType::SignInAlertSent,
                due_alert.security_ref,
            )
            .await?;
            alert_tally.sent_count += 1;
        } else if due_alert.is_past_retry_window {
            self.postgres_sign_in_alert_source
                .delete(&mut *connection, due_alert.id)
                .await?;
            self.record(
                connection,
                SecurityEventType::SignInAlertFailed,
                due_alert.security_ref,
            )
            .await?;
            tracing::error!("a sign-in alert could not be sent within an hour and was dropped");
            alert_tally.dropped_count += 1;
        } else {
            self.postgres_sign_in_alert_source
                .postpone(
                    &mut *connection,
                    due_alert.id,
                    SIGN_IN_ALERT_RETRY_AFTER_SECONDS,
                )
                .await?;
            tracing::warn!("a sign-in alert could not be sent and will be retried");
            alert_tally.retry_count += 1;
        }
        Ok(())
    }

    async fn record(
        &self,
        connection: &mut PgConnection,
        event_type: SecurityEventType,
        security_ref: Uuid,
    ) -> Result<(), SignInAlertError> {
        self.record_security_event
            .execute(
                connection,
                &NewSecurityEvent {
                    event_type,
                    subject_ref: Some(security_ref),
                    client_ip: None,
                    user_agent: None,
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        Ok(())
    }
}
