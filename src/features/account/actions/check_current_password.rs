use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::security::password_hash::verify_password;
use crate::features::account::error::AccountError;
use crate::features::throttling::actions::check_attempt_allowed::CheckAttemptAllowed;
use crate::features::throttling::actions::clear_account_ip_throttle::ClearAccountIpThrottle;
use crate::features::throttling::actions::record_failed_attempt::RecordFailedAttempt;
use crate::features::throttling::entities::throttle_attempt::{
    ThrottleAttempt, ThrottleScope, email_account_key,
};
use crate::features::users::entities::user::User;

pub struct CheckCurrentPassword<'a> {
    check_attempt_allowed: CheckAttemptAllowed<'a>,
    record_failed_attempt: RecordFailedAttempt<'a>,
    clear_account_ip_throttle: ClearAccountIpThrottle<'a>,
}

impl<'a> CheckCurrentPassword<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            check_attempt_allowed: CheckAttemptAllowed::new(db_pool),
            record_failed_attempt: RecordFailedAttempt::new(db_pool),
            clear_account_ip_throttle: ClearAccountIpThrottle::new(db_pool),
        }
    }

    pub async fn execute(
        &self,
        signed_in_user: &User,
        entered_password: Zeroizing<String>,
        request_context: &RequestContext,
    ) -> Result<bool, AccountError> {
        let throttle_attempt = ThrottleAttempt {
            scope: ThrottleScope::SignIn,
            account_key: email_account_key(&signed_in_user.email),
            client_ip: request_context.client_ip,
            has_valid_remembered_device: false,
        };
        self.check_attempt_allowed
            .execute(&throttle_attempt)
            .await?;
        let password_verification =
            verify_password(entered_password, signed_in_user.password_hash.clone()).await?;
        if !password_verification.is_valid {
            self.record_failed_attempt
                .execute(&throttle_attempt)
                .await?;
            return Ok(false);
        }
        self.clear_account_ip_throttle
            .execute(&throttle_attempt)
            .await?;
        Ok(true)
    }
}
