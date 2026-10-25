use sqlx::PgPool;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::ThrottleAttempt;
use crate::features::throttling::error::ThrottleError;

pub struct ClearAccountIpThrottle<'a> {
    postgres_throttle_source: PostgresThrottleSource<'a>,
}

impl<'a> ClearAccountIpThrottle<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource::new(db_pool),
        }
    }

    pub async fn execute(&self, throttle_attempt: &ThrottleAttempt) -> Result<(), ThrottleError> {
        Ok(self
            .postgres_throttle_source
            .delete(&throttle_attempt.account_ip_key())
            .await?)
    }
}
