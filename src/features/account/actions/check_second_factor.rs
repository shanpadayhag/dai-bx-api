use sqlx::{PgConnection, PgPool};

use crate::core::request_context::RequestContext;
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::features::account::error::AccountError;
use crate::features::auth::actions::accept_second_factor::{
    AcceptSecondFactor, AcceptedSecondFactor,
};
use crate::features::throttling::actions::check_attempt_allowed::CheckAttemptAllowed;
use crate::features::throttling::actions::clear_account_ip_throttle::ClearAccountIpThrottle;
use crate::features::throttling::actions::record_failed_attempt::RecordFailedAttempt;
use crate::features::throttling::entities::throttle_attempt::{
    ThrottleAttempt, ThrottleScope, email_account_key,
};
use crate::features::users::entities::user::User;

pub struct CheckSecondFactor<'a> {
    accept_second_factor: AcceptSecondFactor<'a>,
    check_attempt_allowed: CheckAttemptAllowed<'a>,
    record_failed_attempt: RecordFailedAttempt<'a>,
    clear_account_ip_throttle: ClearAccountIpThrottle<'a>,
}

impl<'a> CheckSecondFactor<'a> {
    pub fn new(db_pool: &'a PgPool, totp_keyring: &'a TotpKeyring) -> Self {
        Self {
            accept_second_factor: AcceptSecondFactor::new(totp_keyring),
            check_attempt_allowed: CheckAttemptAllowed::new(db_pool),
            record_failed_attempt: RecordFailedAttempt::new(db_pool),
            clear_account_ip_throttle: ClearAccountIpThrottle::new(db_pool),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        signed_in_user: &User,
        entered_code: &str,
        request_context: &RequestContext,
    ) -> Result<AcceptedSecondFactor, AccountError> {
        let throttle_attempt = ThrottleAttempt {
            scope: ThrottleScope::CodeEntry,
            account_key: email_account_key(&signed_in_user.email),
            client_ip: request_context.client_ip,
            has_valid_remembered_device: false,
        };
        self.check_attempt_allowed
            .execute(&throttle_attempt)
            .await?;
        let Some(accepted_factor) = self
            .accept_second_factor
            .execute(connection, signed_in_user.id, entered_code)
            .await?
        else {
            self.record_failed_attempt
                .execute(&throttle_attempt)
                .await?;
            return Err(AccountError::WrongCode);
        };
        self.clear_account_ip_throttle
            .execute(&throttle_attempt)
            .await?;
        Ok(accepted_factor)
    }
}
