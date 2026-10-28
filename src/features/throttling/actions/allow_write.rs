use sqlx::PgPool;
use uuid::Uuid;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    WRITE_LIMIT, WRITE_WINDOW_SECONDS, write_key,
};
use crate::features::throttling::error::ThrottleError;

pub struct AllowWrite<'a> {
    postgres_throttle_source: PostgresThrottleSource<'a>,
}

impl<'a> AllowWrite<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource::new(db_pool),
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<(), ThrottleError> {
        let write_count = self
            .postgres_throttle_source
            .record_windowed_failure(&write_key(user_id), WRITE_LIMIT, WRITE_WINDOW_SECONDS)
            .await?;
        match write_count.blocked_seconds {
            Some(blocked_seconds) if write_count.failures > WRITE_LIMIT => {
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

    async fn write_outcomes(db_pool: &PgPool, user_id: Uuid, write_count: i32) -> Vec<bool> {
        let allow_write = AllowWrite::new(db_pool);
        let mut outcomes = Vec::with_capacity(usize::try_from(write_count).unwrap_or(0));
        for _ in 0..write_count {
            outcomes.push(allow_write.execute(user_id).await.is_ok());
        }
        outcomes
    }

    #[sqlx::test]
    async fn allows_600_writes_in_a_window_and_refuses_the_601st(db_pool: PgPool) {
        let user_id = Uuid::now_v7();

        let outcomes = write_outcomes(&db_pool, user_id, WRITE_LIMIT + 1).await;

        assert!(outcomes[..600].iter().all(|is_allowed| *is_allowed));
        assert!(!outcomes[600]);
    }

    #[sqlx::test]
    async fn counts_each_person_s_writes_separately(db_pool: PgPool) {
        let busy_user_id = Uuid::now_v7();
        write_outcomes(&db_pool, busy_user_id, WRITE_LIMIT + 1).await;

        let quiet_user_outcome = AllowWrite::new(&db_pool).execute(Uuid::now_v7()).await;

        assert!(quiet_user_outcome.is_ok());
    }

    #[sqlx::test]
    async fn starts_a_fresh_window_after_ten_minutes(db_pool: PgPool) {
        let user_id = Uuid::now_v7();
        write_outcomes(&db_pool, user_id, WRITE_LIMIT + 1).await;
        sqlx::query(
            "UPDATE auth_throttles
             SET window_started_at = now() - interval '601 seconds',
                 blocked_until = now() - interval '1 second'
             WHERE key = $1",
        )
        .bind(write_key(user_id))
        .execute(&db_pool)
        .await
        .expect("aging the window should succeed");

        let next_outcome = AllowWrite::new(&db_pool).execute(user_id).await;

        assert!(next_outcome.is_ok());
    }
}
