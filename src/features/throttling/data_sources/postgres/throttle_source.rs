use sqlx::PgPool;

use crate::features::throttling::entities::throttle_attempt::{
    FIRST_BLOCK_SECONDS, FREE_ACCOUNT_IP_FAILURES, MAX_BLOCK_SECONDS,
};

const MAX_DOUBLING_EXPONENT: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrottleCount {
    pub failures: i32,
    pub blocked_seconds: Option<i64>,
}

pub struct PostgresThrottleSource<'a> {
    db_pool: &'a PgPool,
}

impl<'a> PostgresThrottleSource<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn find_longest_block_seconds(
        &self,
        throttle_keys: &[String],
    ) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT max(ceil(extract(epoch FROM blocked_until - now())))::bigint AS "blocked_seconds"
               FROM auth_throttles
               WHERE key = ANY($1) AND blocked_until > now()"#,
            throttle_keys,
        )
        .fetch_one(self.db_pool)
        .await
    }

    pub async fn record_escalating_failure(
        &self,
        throttle_key: &str,
    ) -> Result<ThrottleCount, sqlx::Error> {
        sqlx::query_as!(
            ThrottleCount,
            r#"INSERT INTO auth_throttles (key, failures, window_started_at, blocked_until)
               VALUES ($1, 1, now(), NULL)
               ON CONFLICT (key) DO UPDATE SET
                   failures = auth_throttles.failures + 1,
                   blocked_until = CASE
                       WHEN auth_throttles.failures + 1 > $2 THEN now() + make_interval(
                           secs => LEAST(
                               $4::float8,
                               $3::float8 * power(2, LEAST(auth_throttles.failures + 1 - $2 - 1, $5))
                           )
                       )
                       ELSE auth_throttles.blocked_until
                   END
               RETURNING failures,
                         CASE WHEN blocked_until > now()
                              THEN ceil(extract(epoch FROM blocked_until - now()))::bigint
                         END AS "blocked_seconds""#,
            throttle_key,
            FREE_ACCOUNT_IP_FAILURES,
            FIRST_BLOCK_SECONDS as f64,
            MAX_BLOCK_SECONDS as f64,
            MAX_DOUBLING_EXPONENT,
        )
        .fetch_one(self.db_pool)
        .await
    }

    pub async fn record_windowed_failure(
        &self,
        throttle_key: &str,
        failure_limit: i32,
        window_seconds: i64,
    ) -> Result<ThrottleCount, sqlx::Error> {
        sqlx::query_as!(
            ThrottleCount,
            r#"INSERT INTO auth_throttles (key, failures, window_started_at, blocked_until)
               VALUES (
                   $1, 1, now(),
                   CASE WHEN 1 >= $2 THEN now() + make_interval(secs => $3::float8) END
               )
               ON CONFLICT (key) DO UPDATE SET
                   failures = CASE
                       WHEN auth_throttles.window_started_at <= now() - make_interval(secs => $3::float8) THEN 1
                       ELSE auth_throttles.failures + 1
                   END,
                   window_started_at = CASE
                       WHEN auth_throttles.window_started_at <= now() - make_interval(secs => $3::float8) THEN now()
                       ELSE auth_throttles.window_started_at
                   END,
                   blocked_until = CASE
                       WHEN auth_throttles.window_started_at <= now() - make_interval(secs => $3::float8) THEN
                           CASE WHEN 1 >= $2 THEN now() + make_interval(secs => $3::float8) END
                       WHEN auth_throttles.failures + 1 >= $2 THEN
                           auth_throttles.window_started_at + make_interval(secs => $3::float8)
                   END
               RETURNING failures,
                         CASE WHEN blocked_until > now()
                              THEN ceil(extract(epoch FROM blocked_until - now()))::bigint
                         END AS "blocked_seconds""#,
            throttle_key,
            failure_limit,
            window_seconds as f64,
        )
        .fetch_one(self.db_pool)
        .await
    }

    pub async fn delete(&self, throttle_key: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM auth_throttles WHERE key = $1", throttle_key)
            .execute(self.db_pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::throttling::entities::throttle_attempt::account_ip_block_seconds;

    async fn age_window(db_pool: &PgPool, throttle_key: &str, seconds_ago: i64) {
        sqlx::query(
            "UPDATE auth_throttles
             SET window_started_at = now() - make_interval(secs => $2::float8)
             WHERE key = $1",
        )
        .bind(throttle_key)
        .bind(seconds_ago as f64)
        .execute(db_pool)
        .await
        .expect("aging the window should succeed");
    }

    fn assert_close(actual_seconds: Option<i64>, expected_seconds: Option<i64>) {
        match (actual_seconds, expected_seconds) {
            (None, None) => {}
            (Some(actual), Some(expected)) => assert!(
                (expected - 2..=expected).contains(&actual),
                "expected about {expected} seconds, got {actual}"
            ),
            mismatch => panic!("block mismatch: {mismatch:?}"),
        }
    }

    #[sqlx::test]
    async fn escalates_blocks_exactly_like_the_rule(db_pool: PgPool) {
        let throttle_source = PostgresThrottleSource::new(&db_pool);

        for expected_failures in 1..=12 {
            let throttle_count = throttle_source
                .record_escalating_failure("signin:acct_ip:test:1.2.3.4")
                .await
                .expect("recording should succeed");

            assert_eq!(throttle_count.failures, expected_failures);
            assert_close(
                throttle_count.blocked_seconds,
                account_ip_block_seconds(expected_failures),
            );
        }
    }

    #[sqlx::test]
    async fn survives_a_very_long_failure_streak_at_the_cap(db_pool: PgPool) {
        let throttle_source = PostgresThrottleSource::new(&db_pool);
        let mut last_throttle_count = None;

        for _ in 0..80 {
            last_throttle_count = Some(
                throttle_source
                    .record_escalating_failure("signin:acct_ip:streak:1.2.3.4")
                    .await
                    .expect("recording should succeed"),
            );
        }

        assert_close(
            last_throttle_count.and_then(|throttle_count| throttle_count.blocked_seconds),
            Some(MAX_BLOCK_SECONDS),
        );
    }

    #[sqlx::test]
    async fn blocks_a_windowed_key_at_its_limit_until_the_window_ends(db_pool: PgPool) {
        let throttle_source = PostgresThrottleSource::new(&db_pool);

        let below_limit_count = throttle_source
            .record_windowed_failure("email_send:verify_email:k", 3, 3600)
            .await
            .expect("recording should succeed");
        throttle_source
            .record_windowed_failure("email_send:verify_email:k", 3, 3600)
            .await
            .expect("recording should succeed");
        let at_limit_count = throttle_source
            .record_windowed_failure("email_send:verify_email:k", 3, 3600)
            .await
            .expect("recording should succeed");

        assert_eq!(below_limit_count.blocked_seconds, None);
        assert_eq!(at_limit_count.failures, 3);
        assert_close(at_limit_count.blocked_seconds, Some(3600));
    }

    #[sqlx::test]
    async fn starts_a_fresh_window_once_the_old_one_has_passed(db_pool: PgPool) {
        let throttle_source = PostgresThrottleSource::new(&db_pool);
        for _ in 0..3 {
            throttle_source
                .record_windowed_failure("signin:ip:1.2.3.4", 3, 3600)
                .await
                .expect("recording should succeed");
        }
        age_window(&db_pool, "signin:ip:1.2.3.4", 3601).await;

        let fresh_window_count = throttle_source
            .record_windowed_failure("signin:ip:1.2.3.4", 3, 3600)
            .await
            .expect("recording should succeed");

        assert_eq!(fresh_window_count.failures, 1);
        assert_eq!(fresh_window_count.blocked_seconds, None);
    }

    #[sqlx::test]
    async fn finds_the_longest_active_block_among_keys_and_deletes_a_key(db_pool: PgPool) {
        let throttle_source = PostgresThrottleSource::new(&db_pool);
        for _ in 0..7 {
            throttle_source
                .record_escalating_failure("long")
                .await
                .expect("recording should succeed");
        }
        for _ in 0..6 {
            throttle_source
                .record_escalating_failure("short")
                .await
                .expect("recording should succeed");
        }
        let keys = [
            String::from("long"),
            String::from("short"),
            String::from("absent"),
        ];

        assert_close(
            throttle_source
                .find_longest_block_seconds(&keys)
                .await
                .expect("lookup should succeed"),
            Some(120),
        );
        throttle_source
            .delete("long")
            .await
            .expect("delete should succeed");
        assert_close(
            throttle_source
                .find_longest_block_seconds(&keys)
                .await
                .expect("lookup should succeed"),
            Some(60),
        );
        assert_eq!(
            throttle_source
                .find_longest_block_seconds(&[String::from("absent")])
                .await
                .expect("lookup should succeed"),
            None
        );
    }
}
