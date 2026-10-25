use sqlx::PgPool;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    EMAIL_SEND_LIMIT, EMAIL_SEND_WINDOW_SECONDS, EmailSendPurpose, email_send_key,
};
use crate::features::throttling::error::ThrottleError;

pub struct AllowEmailSend<'a> {
    postgres_throttle_source: PostgresThrottleSource<'a>,
}

impl<'a> AllowEmailSend<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource::new(db_pool),
        }
    }

    pub async fn execute(
        &self,
        purpose: EmailSendPurpose,
        email: &str,
    ) -> Result<(), ThrottleError> {
        let email_send_key = email_send_key(purpose, email);
        if let Some(blocked_seconds) = self
            .postgres_throttle_source
            .find_longest_block_seconds(std::slice::from_ref(&email_send_key))
            .await?
        {
            return Err(ThrottleError::Blocked {
                retry_after_seconds: u64::try_from(blocked_seconds).unwrap_or(1),
            });
        }
        self.postgres_throttle_source
            .record_windowed_failure(&email_send_key, EMAIL_SEND_LIMIT, EMAIL_SEND_WINDOW_SECONDS)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn try_send(db_pool: &PgPool, purpose: EmailSendPurpose, email: &str) -> Result<(), u64> {
        AllowEmailSend::new(db_pool)
            .execute(purpose, email)
            .await
            .map_err(|throttle_error| match throttle_error {
                ThrottleError::Blocked {
                    retry_after_seconds,
                } => retry_after_seconds,
                ThrottleError::Database(database_error) => {
                    panic!("unexpected error: {database_error}")
                }
            })
    }

    #[sqlx::test]
    async fn allows_three_emails_an_hour_and_refuses_the_fourth(db_pool: PgPool) {
        for _ in 0..3 {
            assert_eq!(
                try_send(
                    &db_pool,
                    EmailSendPurpose::VerifyEmail,
                    "person@example.com"
                )
                .await,
                Ok(())
            );
        }

        let fourth_send = try_send(
            &db_pool,
            EmailSendPurpose::VerifyEmail,
            "Person@Example.com",
        )
        .await;

        assert!(fourth_send.is_err_and(|retry_after_seconds| retry_after_seconds > 3500));
    }

    #[sqlx::test]
    async fn counts_each_purpose_and_address_separately(db_pool: PgPool) {
        for _ in 0..3 {
            try_send(
                &db_pool,
                EmailSendPurpose::VerifyEmail,
                "person@example.com",
            )
            .await
            .expect("sends under the limit should pass");
        }

        assert_eq!(
            try_send(
                &db_pool,
                EmailSendPurpose::PasswordReset,
                "person@example.com"
            )
            .await,
            Ok(())
        );
        assert_eq!(
            try_send(&db_pool, EmailSendPurpose::VerifyEmail, "other@example.com").await,
            Ok(())
        );
    }
}
