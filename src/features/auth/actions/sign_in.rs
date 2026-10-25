use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::security::password_hash::{
    hash_password, verify_against_dummy_hash, verify_password,
};
use crate::core::state::AppState;
use crate::features::auth::actions::challenge_after_password::ChallengeAfterPassword;
use crate::features::auth::error::AuthError;
use crate::features::remembered_devices::actions::find_remembered_device::FindRememberedDevice;
use crate::features::remembered_devices::actions::mark_remembered_device_used::MarkRememberedDeviceUsed;
use crate::features::remembered_devices::entities::remembered_device::RememberedDevice;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::create_session::CreateSession;
use crate::features::sessions::entities::session::IssuedSessionTokens;
use crate::features::throttling::actions::check_attempt_allowed::CheckAttemptAllowed;
use crate::features::throttling::actions::clear_account_ip_throttle::ClearAccountIpThrottle;
use crate::features::throttling::actions::record_failed_attempt::RecordFailedAttempt;
use crate::features::throttling::entities::throttle_attempt::{
    ThrottleAttempt, ThrottleScope, email_account_key,
};
use crate::features::users::actions::find_user_by_email::FindUserByEmail;
use crate::features::users::actions::update_password_hash::UpdatePasswordHash;
use crate::features::users::entities::user::User;

pub struct SignInDetails<'a> {
    pub email: &'a str,
    pub password: Zeroizing<String>,
    pub device_token: Option<&'a str>,
}

pub enum SignInOutcome {
    VerifyEmail,
    SetUpAuthenticator { challenge_token: Zeroizing<String> },
    EnterCode { challenge_token: Zeroizing<String> },
    SignedIn { session_tokens: IssuedSessionTokens },
}

pub struct SignIn<'a> {
    db_pool: &'a PgPool,
    find_user_by_email: FindUserByEmail<'a>,
    find_remembered_device: FindRememberedDevice,
    mark_remembered_device_used: MarkRememberedDeviceUsed,
    check_attempt_allowed: CheckAttemptAllowed<'a>,
    record_failed_attempt: RecordFailedAttempt<'a>,
    clear_account_ip_throttle: ClearAccountIpThrottle<'a>,
    update_password_hash: UpdatePasswordHash,
    challenge_after_password: ChallengeAfterPassword<'a>,
    create_session: CreateSession,
    record_security_event: RecordSecurityEvent,
}

impl<'a> SignIn<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let db_pool = &app_state.db_pool;
        Self {
            db_pool,
            find_user_by_email: FindUserByEmail::new(db_pool),
            find_remembered_device: FindRememberedDevice::new(),
            mark_remembered_device_used: MarkRememberedDeviceUsed::new(),
            check_attempt_allowed: CheckAttemptAllowed::new(db_pool),
            record_failed_attempt: RecordFailedAttempt::new(db_pool),
            clear_account_ip_throttle: ClearAccountIpThrottle::new(db_pool),
            update_password_hash: UpdatePasswordHash::new(),
            challenge_after_password: ChallengeAfterPassword::new(
                &app_state.app_config.totp_keyring,
            ),
            create_session: CreateSession::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        sign_in_details: SignInDetails<'_>,
        request_context: &RequestContext,
    ) -> Result<SignInOutcome, AuthError> {
        let found_user = self
            .find_user_by_email
            .execute(sign_in_details.email)
            .await?;
        let remembered_device = self
            .remembered_device_for(sign_in_details.device_token, found_user.as_ref())
            .await?;
        let throttle_attempt = ThrottleAttempt {
            scope: ThrottleScope::SignIn,
            account_key: email_account_key(sign_in_details.email),
            client_ip: request_context.client_ip,
            has_valid_remembered_device: remembered_device.is_some(),
        };
        self.check_attempt_allowed
            .execute(&throttle_attempt)
            .await?;

        let Some(found_user) = found_user else {
            verify_against_dummy_hash(sign_in_details.password).await;
            return self.refuse(&throttle_attempt, None, request_context).await;
        };
        let password_verification = verify_password(
            sign_in_details.password.clone(),
            found_user.password_hash.clone(),
        )
        .await?;
        if !password_verification.is_valid {
            return self
                .refuse(&throttle_attempt, Some(&found_user), request_context)
                .await;
        }
        self.clear_account_ip_throttle
            .execute(&throttle_attempt)
            .await?;
        if password_verification.needs_rehash {
            let updated_password_hash = hash_password(sign_in_details.password).await?;
            self.update_password_hash
                .execute(self.db_pool, found_user.id, &updated_password_hash)
                .await?;
        }

        let Some(remembered_device) =
            remembered_device.filter(|_| found_user.is_email_verified && found_user.is_mfa_enabled)
        else {
            return self
                .challenge_after_password
                .execute(self.db_pool, &found_user)
                .await;
        };

        let mut sign_in_transaction = self.db_pool.begin().await?;
        let session_tokens = self
            .create_session
            .execute(&mut sign_in_transaction, found_user.id, request_context)
            .await?;
        self.mark_remembered_device_used
            .execute(&mut *sign_in_transaction, remembered_device.id)
            .await?;
        self.record_security_event
            .execute(
                &mut *sign_in_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::SignInSucceeded,
                    subject_ref: Some(found_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_text("method", "remembered_device")?,
                },
            )
            .await?;
        sign_in_transaction.commit().await?;
        Ok(SignInOutcome::SignedIn { session_tokens })
    }

    async fn remembered_device_for(
        &self,
        device_token: Option<&str>,
        found_user: Option<&User>,
    ) -> Result<Option<RememberedDevice>, AuthError> {
        let (Some(device_token), Some(found_user)) = (device_token, found_user) else {
            return Ok(None);
        };
        Ok(self
            .find_remembered_device
            .execute(self.db_pool, device_token)
            .await?
            .filter(|remembered_device| remembered_device.user_id == found_user.id))
    }

    async fn refuse(
        &self,
        throttle_attempt: &ThrottleAttempt,
        found_user: Option<&User>,
        request_context: &RequestContext,
    ) -> Result<SignInOutcome, AuthError> {
        self.record_failed_attempt.execute(throttle_attempt).await?;
        self.record_security_event
            .execute(
                self.db_pool,
                &NewSecurityEvent {
                    event_type: SecurityEventType::SignInFailed,
                    subject_ref: found_user.map(|refused_user| refused_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default().with_text("stage", "password")?,
                },
            )
            .await?;
        Err(AuthError::InvalidCredentials)
    }
}
