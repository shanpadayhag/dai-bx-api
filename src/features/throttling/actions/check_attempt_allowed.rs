use sqlx::PgPool;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::ThrottleAttempt;
use crate::features::throttling::error::ThrottleError;

pub struct CheckAttemptAllowed<'a> {
    postgres_throttle_source: PostgresThrottleSource<'a>,
}

impl<'a> CheckAttemptAllowed<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource::new(db_pool),
        }
    }

    pub async fn execute(&self, throttle_attempt: &ThrottleAttempt) -> Result<(), ThrottleError> {
        let mut checked_keys = vec![throttle_attempt.account_ip_key()];
        checked_keys.extend(throttle_attempt.ip_key());
        if !throttle_attempt.has_valid_remembered_device {
            checked_keys.push(throttle_attempt.account_key_name());
        }
        match self
            .postgres_throttle_source
            .find_longest_block_seconds(&checked_keys)
            .await?
        {
            Some(blocked_seconds) => Err(ThrottleError::Blocked {
                retry_after_seconds: u64::try_from(blocked_seconds).unwrap_or(1),
            }),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;
    use crate::features::throttling::actions::clear_account_ip_throttle::ClearAccountIpThrottle;
    use crate::features::throttling::actions::record_failed_attempt::RecordFailedAttempt;
    use crate::features::throttling::entities::throttle_attempt::{
        LOOSE_FAILURE_LIMIT, ThrottleScope, email_account_key,
    };

    fn sign_in_attempt(email: &str, client_ip: &str) -> ThrottleAttempt {
        ThrottleAttempt {
            scope: ThrottleScope::SignIn,
            account_key: email_account_key(email),
            client_ip: Some(client_ip.parse::<IpAddr>().expect("valid address")),
            has_valid_remembered_device: false,
        }
    }

    async fn fail(db_pool: &PgPool, throttle_attempt: &ThrottleAttempt, failure_count: i32) {
        for _ in 0..failure_count {
            RecordFailedAttempt::new(db_pool)
                .execute(throttle_attempt)
                .await
                .expect("recording should succeed");
        }
    }

    async fn retry_after(db_pool: &PgPool, throttle_attempt: &ThrottleAttempt) -> Option<u64> {
        match CheckAttemptAllowed::new(db_pool)
            .execute(throttle_attempt)
            .await
        {
            Ok(()) => None,
            Err(ThrottleError::Blocked {
                retry_after_seconds,
            }) => Some(retry_after_seconds),
            Err(database_error) => panic!("unexpected error: {database_error}"),
        }
    }

    fn assert_within_last_minute_of_the_window(actual_seconds: Option<u64>) {
        let blocked_seconds = actual_seconds.expect("the attempt should be blocked");
        assert!(
            (3540..=3600).contains(&blocked_seconds),
            "expected the rest of a one-hour window, got {blocked_seconds}"
        );
    }

    fn assert_about(actual_seconds: Option<u64>, expected_seconds: u64) {
        let blocked_seconds = actual_seconds.expect("the attempt should be blocked");
        assert!(
            (expected_seconds - 2..=expected_seconds).contains(&blocked_seconds),
            "expected about {expected_seconds} seconds, got {blocked_seconds}"
        );
    }

    #[sqlx::test]
    async fn allows_five_failures_then_blocks_a_minute_then_two(db_pool: PgPool) {
        let throttle_attempt = sign_in_attempt("person@example.com", "180.191.77.129");

        fail(&db_pool, &throttle_attempt, 5).await;
        assert_eq!(retry_after(&db_pool, &throttle_attempt).await, None);

        fail(&db_pool, &throttle_attempt, 1).await;
        assert_about(retry_after(&db_pool, &throttle_attempt).await, 60);

        fail(&db_pool, &throttle_attempt, 1).await;
        assert_about(retry_after(&db_pool, &throttle_attempt).await, 120);
    }

    #[sqlx::test]
    async fn caps_the_block_at_fifteen_minutes(db_pool: PgPool) {
        let throttle_attempt = sign_in_attempt("person@example.com", "180.191.77.129");

        fail(&db_pool, &throttle_attempt, 20).await;

        assert_about(retry_after(&db_pool, &throttle_attempt).await, 900);
    }

    #[sqlx::test]
    async fn lets_the_account_and_ip_through_again_after_a_success(db_pool: PgPool) {
        let throttle_attempt = sign_in_attempt("person@example.com", "180.191.77.129");
        fail(&db_pool, &throttle_attempt, 6).await;

        ClearAccountIpThrottle::new(&db_pool)
            .execute(&throttle_attempt)
            .await
            .expect("clearing should succeed");

        assert_eq!(retry_after(&db_pool, &throttle_attempt).await, None);
    }

    #[sqlx::test]
    async fn blocks_one_ip_without_blocking_the_same_account_elsewhere(db_pool: PgPool) {
        let attacking_attempt = sign_in_attempt("person@example.com", "6.6.6.6");
        fail(&db_pool, &attacking_attempt, 10).await;

        let owner_attempt = sign_in_attempt("person@example.com", "180.191.77.129");

        assert_eq!(retry_after(&db_pool, &owner_attempt).await, None);
    }

    #[sqlx::test]
    async fn blocks_an_account_attacked_from_many_ips_unless_the_device_is_remembered(
        db_pool: PgPool,
    ) {
        for attacker_index in 0..LOOSE_FAILURE_LIMIT {
            let attacker_ip = format!("10.0.{}.{}", attacker_index / 250, attacker_index % 250 + 1);
            fail(
                &db_pool,
                &sign_in_attempt("person@example.com", &attacker_ip),
                1,
            )
            .await;
        }
        let owner_attempt = sign_in_attempt("person@example.com", "180.191.77.129");
        let remembered_owner_attempt = ThrottleAttempt {
            has_valid_remembered_device: true,
            ..owner_attempt.clone()
        };

        assert_within_last_minute_of_the_window(retry_after(&db_pool, &owner_attempt).await);
        assert_eq!(retry_after(&db_pool, &remembered_owner_attempt).await, None);
    }

    #[sqlx::test]
    async fn blocks_an_ip_trying_many_accounts(db_pool: PgPool) {
        for account_index in 0..LOOSE_FAILURE_LIMIT {
            let tried_email = format!("person{account_index}@example.com");
            fail(&db_pool, &sign_in_attempt(&tried_email, "6.6.6.6"), 1).await;
        }

        let next_account_attempt = sign_in_attempt("fresh@example.com", "6.6.6.6");

        assert_within_last_minute_of_the_window(retry_after(&db_pool, &next_account_attempt).await);
    }

    #[sqlx::test]
    async fn throttles_an_unknown_email_exactly_like_a_real_one(db_pool: PgPool) {
        let unknown_email_attempt =
            sign_in_attempt("nobody-has-this@example.com", "180.191.77.129");

        fail(&db_pool, &unknown_email_attempt, 6).await;

        assert_about(retry_after(&db_pool, &unknown_email_attempt).await, 60);
    }

    #[sqlx::test]
    async fn keeps_sign_in_and_code_entry_throttles_apart(db_pool: PgPool) {
        let sign_in_attempt = sign_in_attempt("person@example.com", "180.191.77.129");
        fail(&db_pool, &sign_in_attempt, 6).await;

        let code_entry_attempt = ThrottleAttempt {
            scope: ThrottleScope::CodeEntry,
            ..sign_in_attempt
        };

        assert_eq!(retry_after(&db_pool, &code_entry_attempt).await, None);
    }
}
