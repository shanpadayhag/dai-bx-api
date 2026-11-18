use std::net::IpAddr;

use sqlx::PgConnection;

use crate::features::throttling::data_sources::postgres::throttle_source::PostgresThrottleSource;
use crate::features::throttling::entities::throttle_attempt::{
    EMAIL_CAP_KEY, EMAIL_REQUEST_IP_LIMIT, EMAIL_REQUEST_IP_WINDOW_SECONDS, EMAIL_SEND_LIMIT,
    EMAIL_SEND_WINDOW_SECONDS, EmailSendPurpose, email_request_ip_key, email_send_key,
};
use crate::features::throttling::error::ThrottleError;

pub struct AllowEmailSend {
    postgres_throttle_source: PostgresThrottleSource,
}

impl Default for AllowEmailSend {
    fn default() -> Self {
        Self::new()
    }
}

impl AllowEmailSend {
    pub fn new() -> Self {
        Self {
            postgres_throttle_source: PostgresThrottleSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        purpose: EmailSendPurpose,
        email: &str,
        limited_client_ip: Option<IpAddr>,
    ) -> Result<(), ThrottleError> {
        let email_send_key = email_send_key(purpose, email);
        let client_ip_key = limited_client_ip.map(email_request_ip_key);
        let mut checked_keys = vec![EMAIL_CAP_KEY.to_owned(), email_send_key.clone()];
        checked_keys.extend(client_ip_key.clone());
        if let Some(blocked_seconds) = self
            .postgres_throttle_source
            .find_longest_block_seconds(&mut *connection, &checked_keys)
            .await?
        {
            return Err(ThrottleError::Blocked {
                retry_after_seconds: u64::try_from(blocked_seconds).unwrap_or(1),
            });
        }
        self.postgres_throttle_source
            .record_windowed_failure(
                &mut *connection,
                &email_send_key,
                EMAIL_SEND_LIMIT,
                EMAIL_SEND_WINDOW_SECONDS,
            )
            .await?;
        if let Some(client_ip_key) = client_ip_key {
            self.postgres_throttle_source
                .record_windowed_failure(
                    &mut *connection,
                    &client_ip_key,
                    EMAIL_REQUEST_IP_LIMIT,
                    EMAIL_REQUEST_IP_WINDOW_SECONDS,
                )
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::throttling::actions::count_account_email::CountAccountEmail;
    use crate::features::throttling::entities::throttle_attempt::EMAIL_CAP_LIMIT;

    const CLIENT_IP: &str = "180.191.77.129";

    fn client_ip() -> Option<IpAddr> {
        Some(CLIENT_IP.parse().expect("valid address"))
    }

    async fn try_send_from(
        db_pool: &PgPool,
        purpose: EmailSendPurpose,
        email: &str,
        limited_client_ip: Option<IpAddr>,
    ) -> Result<(), u64> {
        let mut connection = db_pool
            .acquire()
            .await
            .expect("a connection should be free");
        AllowEmailSend::new()
            .execute(&mut connection, purpose, email, limited_client_ip)
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

    async fn try_send(db_pool: &PgPool, purpose: EmailSendPurpose, email: &str) -> Result<(), u64> {
        try_send_from(db_pool, purpose, email, None).await
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

    #[sqlx::test]
    async fn allows_ten_requests_an_hour_from_one_ip_and_refuses_the_eleventh(db_pool: PgPool) {
        for request_number in 0..10 {
            try_send_from(
                &db_pool,
                EmailSendPurpose::PasswordReset,
                &format!("person{request_number}@example.com"),
                client_ip(),
            )
            .await
            .expect("requests under the limit should pass");
        }

        let eleventh_request = try_send_from(
            &db_pool,
            EmailSendPurpose::VerifyEmail,
            "another@example.com",
            client_ip(),
        )
        .await;

        assert!(eleventh_request.is_err_and(|retry_after_seconds| retry_after_seconds > 3500));
    }

    #[sqlx::test]
    async fn refuses_every_address_once_the_daily_cap_is_full(db_pool: PgPool) {
        let mut connection = db_pool
            .acquire()
            .await
            .expect("a connection should be free");
        for _ in 0..EMAIL_CAP_LIMIT {
            CountAccountEmail::new()
                .execute(&mut connection)
                .await
                .expect("counting should succeed");
        }
        drop(connection);

        let refused_send = try_send(
            &db_pool,
            EmailSendPurpose::ChangeEmail,
            "person@example.com",
        )
        .await;

        assert!(refused_send.is_err_and(|retry_after_seconds| retry_after_seconds > 86_000));
    }
}
