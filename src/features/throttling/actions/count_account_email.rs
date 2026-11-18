use sqlx::PgConnection;

use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    EMAIL_CAP_KEY, EMAIL_CAP_LIMIT, EMAIL_CAP_WINDOW_SECONDS,
};
use crate::features::throttling::error::ThrottleError;

pub struct CountAccountEmail {
    postgres_throttle_source: PostgresThrottleSource,
    record_security_event: RecordSecurityEvent,
}

impl Default for CountAccountEmail {
    fn default() -> Self {
        Self::new()
    }
}

impl CountAccountEmail {
    pub fn new() -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource,
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(&self, connection: &mut PgConnection) -> Result<(), ThrottleError> {
        let email_count = self
            .postgres_throttle_source
            .record_windowed_failure(
                &mut *connection,
                EMAIL_CAP_KEY,
                EMAIL_CAP_LIMIT,
                EMAIL_CAP_WINDOW_SECONDS,
            )
            .await?;
        if email_count.failures == EMAIL_CAP_LIMIT {
            self.record_security_event
                .execute(
                    &mut *connection,
                    &NewSecurityEvent {
                        event_type: SecurityEventType::EmailCapReached,
                        subject_ref: None,
                        client_ip: None,
                        user_agent: None,
                        details: SecurityEventDetails::default(),
                    },
                )
                .await?;
            tracing::error!("the daily account email cap was reached");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    async fn count_emails(db_pool: &PgPool, email_count: i32) {
        let mut connection = db_pool
            .acquire()
            .await
            .expect("a connection should be free");
        for _ in 0..email_count {
            CountAccountEmail::new()
                .execute(&mut connection)
                .await
                .expect("counting should succeed");
        }
    }

    async fn cap_reached_event_count(db_pool: &PgPool) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM security_events WHERE event_type = 'email_cap_reached'",
        )
        .fetch_one(db_pool)
        .await
        .expect("counting events should succeed")
    }

    #[sqlx::test]
    async fn records_nothing_below_the_cap(db_pool: PgPool) {
        count_emails(&db_pool, EMAIL_CAP_LIMIT - 1).await;

        assert_eq!(cap_reached_event_count(&db_pool).await, 0);
    }

    #[sqlx::test]
    async fn records_reaching_the_cap_once_even_when_more_emails_follow(db_pool: PgPool) {
        count_emails(&db_pool, EMAIL_CAP_LIMIT + 3).await;

        assert_eq!(cap_reached_event_count(&db_pool).await, 1);
    }

    #[sqlx::test]
    async fn counts_nothing_when_the_callers_transaction_rolls_back(db_pool: PgPool) {
        let mut transaction = db_pool.begin().await.expect("a transaction should start");
        CountAccountEmail::new()
            .execute(&mut transaction)
            .await
            .expect("counting should succeed");
        transaction
            .rollback()
            .await
            .expect("rolling back should succeed");

        let stored_count: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_throttles")
            .fetch_one(&db_pool)
            .await
            .expect("counting rows should succeed");

        assert_eq!(stored_count, 0);
    }
}
