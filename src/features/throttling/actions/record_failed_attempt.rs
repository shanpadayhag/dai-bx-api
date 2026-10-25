use sqlx::PgPool;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    LOOSE_FAILURE_LIMIT, LOOSE_WINDOW_SECONDS, ThrottleAttempt,
};
use crate::features::throttling::error::ThrottleError;

pub struct RecordFailedAttempt<'a> {
    postgres_throttle_source: PostgresThrottleSource<'a>,
}

impl<'a> RecordFailedAttempt<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource::new(db_pool),
        }
    }

    pub async fn execute(&self, throttle_attempt: &ThrottleAttempt) -> Result<(), ThrottleError> {
        self.postgres_throttle_source
            .record_escalating_failure(&throttle_attempt.account_ip_key())
            .await?;
        self.postgres_throttle_source
            .record_windowed_failure(
                &throttle_attempt.account_key_name(),
                LOOSE_FAILURE_LIMIT,
                LOOSE_WINDOW_SECONDS,
            )
            .await?;
        if let Some(ip_key) = throttle_attempt.ip_key() {
            self.postgres_throttle_source
                .record_windowed_failure(&ip_key, LOOSE_FAILURE_LIMIT, LOOSE_WINDOW_SECONDS)
                .await?;
        }
        Ok(())
    }
}
