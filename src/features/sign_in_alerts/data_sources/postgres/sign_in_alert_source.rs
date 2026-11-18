use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::sign_in_alerts::entities::sign_in_alert::{DueSignInAlert, NewSignInAlert};

#[derive(Default)]
pub struct PostgresSignInAlertSource;

impl PostgresSignInAlertSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_alert: &NewSignInAlert<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO pending_sign_in_alerts (id, user_id, signed_in_at, user_agent, sign_in_method)
             VALUES ($1, $2, now(), $3, $4)",
            Uuid::now_v7(),
            new_alert.user_id,
            new_alert.user_agent,
            new_alert.sign_in_method,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn claim_next_due(
        &self,
        executor: impl PgExecutor<'_>,
        retry_window_seconds: i64,
    ) -> Result<Option<DueSignInAlert>, sqlx::Error> {
        sqlx::query_as!(
            DueSignInAlert,
            r#"SELECT pending_sign_in_alerts.id,
                      users.email::text AS "recipient!",
                      users.security_ref,
                      users.calendar_time_zone,
                      pending_sign_in_alerts.user_agent,
                      pending_sign_in_alerts.sign_in_method,
                      (extract(epoch FROM pending_sign_in_alerts.signed_in_at) * 1000)::bigint
                          AS "signed_in_unix_millis!",
                      pending_sign_in_alerts.signed_in_at
                          < now() - make_interval(secs => $1::float8)
                          AS "is_past_retry_window!"
               FROM pending_sign_in_alerts
               JOIN users ON users.id = pending_sign_in_alerts.user_id
               WHERE pending_sign_in_alerts.next_attempt_at <= now()
               ORDER BY pending_sign_in_alerts.next_attempt_at, pending_sign_in_alerts.id
               LIMIT 1
               FOR UPDATE OF pending_sign_in_alerts SKIP LOCKED"#,
            retry_window_seconds as f64,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        alert_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM pending_sign_in_alerts WHERE id = $1", alert_id)
            .execute(executor)
            .await?;
        Ok(())
    }

    pub async fn postpone(
        &self,
        executor: impl PgExecutor<'_>,
        alert_id: Uuid,
        retry_after_seconds: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE pending_sign_in_alerts
             SET next_attempt_at = now() + make_interval(secs => $2::float8)
             WHERE id = $1",
            alert_id,
            retry_after_seconds as f64,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}
