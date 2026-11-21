use sqlx::PgPool;
use uuid::Uuid;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    TEST_PUSH_LIMIT, TEST_PUSH_WINDOW_SECONDS, test_push_key,
};
use crate::features::throttling::error::ThrottleError;

pub struct AllowTestPush<'a> {
    db_pool: &'a PgPool,
    postgres_throttle_source: PostgresThrottleSource,
}

impl<'a> AllowTestPush<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_throttle_source: PostgresThrottleSource,
        }
    }

    pub async fn execute(&self, device_id: Uuid) -> Result<(), ThrottleError> {
        let test_push_count = self
            .postgres_throttle_source
            .record_windowed_failure(
                self.db_pool,
                &test_push_key(device_id),
                TEST_PUSH_LIMIT,
                TEST_PUSH_WINDOW_SECONDS,
            )
            .await?;
        match test_push_count.blocked_seconds {
            Some(blocked_seconds) if test_push_count.failures > TEST_PUSH_LIMIT => {
                Err(ThrottleError::Blocked {
                    retry_after_seconds: u64::try_from(blocked_seconds).unwrap_or(1),
                })
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn allows_five_test_sends_per_device_and_refuses_the_sixth(db_pool: PgPool) {
        let device_id = Uuid::now_v7();
        let allow_test_push = AllowTestPush::new(&db_pool);
        let mut outcomes = Vec::with_capacity(6);
        for _ in 0..6 {
            outcomes.push(allow_test_push.execute(device_id).await.is_ok());
        }

        let other_device_outcome = allow_test_push.execute(Uuid::now_v7()).await;

        assert_eq!(outcomes, vec![true, true, true, true, true, false]);
        assert!(other_device_outcome.is_ok());
    }
}
