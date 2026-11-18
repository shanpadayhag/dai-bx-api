use sqlx::PgExecutor;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::ThrottleAttempt;
use crate::features::throttling::error::ThrottleError;

pub struct ClearAccountIpThrottle {
    postgres_throttle_source: PostgresThrottleSource,
}

impl Default for ClearAccountIpThrottle {
    fn default() -> Self {
        Self::new()
    }
}

impl ClearAccountIpThrottle {
    pub fn new() -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        throttle_attempt: &ThrottleAttempt,
    ) -> Result<(), ThrottleError> {
        Ok(self
            .postgres_throttle_source
            .delete(executor, &throttle_attempt.account_ip_key())
            .await?)
    }
}
