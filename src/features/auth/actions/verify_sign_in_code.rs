use sqlx::{PgConnection, PgPool};
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::actions::accept_second_factor::AcceptSecondFactor;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::actions::claim_auth_challenge_attempt::ClaimAuthChallengeAttempt;
use crate::features::auth_challenges::actions::delete_auth_challenge::DeleteAuthChallenge;
use crate::features::auth_challenges::actions::find_active_auth_challenge::FindActiveAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::remembered_devices::actions::remember_device::RememberDevice;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::create_session::CreateSession;
use crate::features::sessions::entities::presented_session::PresentedSession;
use crate::features::sessions::entities::session::IssuedSessionTokens;
use crate::features::throttling::actions::check_attempt_allowed::CheckAttemptAllowed;
use crate::features::throttling::actions::clear_account_ip_throttle::ClearAccountIpThrottle;
use crate::features::throttling::actions::record_failed_attempt::RecordFailedAttempt;
use crate::features::throttling::entities::throttle_attempt::{
    ThrottleAttempt, ThrottleScope, email_account_key,
};
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::entities::user::User;

pub struct SignInCodeEntry<'a> {
    pub challenge_token: &'a str,
    pub entered_code: &'a str,
    pub should_remember_device: bool,
}

pub struct VerifiedSignIn {
    pub session_tokens: IssuedSessionTokens,
    pub device_token: Option<Zeroizing<String>>,
    pub remaining_recovery_codes: Option<i64>,
}

pub struct VerifySignInCode<'a> {
    db_pool: &'a PgPool,
    find_active_auth_challenge: FindActiveAuthChallenge,
    claim_auth_challenge_attempt: ClaimAuthChallengeAttempt,
    delete_auth_challenge: DeleteAuthChallenge,
    find_user_by_id: FindUserById,
    accept_second_factor: AcceptSecondFactor<'a>,
    check_attempt_allowed: CheckAttemptAllowed<'a>,
    record_failed_attempt: RecordFailedAttempt<'a>,
    clear_account_ip_throttle: ClearAccountIpThrottle<'a>,
    create_session: CreateSession,
    remember_device: RememberDevice,
    record_security_event: RecordSecurityEvent,
}

impl<'a> VerifySignInCode<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let db_pool = &app_state.db_pool;
        Self {
            db_pool,
            find_active_auth_challenge: FindActiveAuthChallenge::new(),
            claim_auth_challenge_attempt: ClaimAuthChallengeAttempt::new(),
            delete_auth_challenge: DeleteAuthChallenge::new(),
            find_user_by_id: FindUserById::new(),
            accept_second_factor: AcceptSecondFactor::new(&app_state.app_config.totp_keyring),
            check_attempt_allowed: CheckAttemptAllowed::new(db_pool),
            record_failed_attempt: RecordFailedAttempt::new(db_pool),
            clear_account_ip_throttle: ClearAccountIpThrottle::new(db_pool),
            create_session: CreateSession::new(),
            remember_device: RememberDevice::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        code_entry: SignInCodeEntry<'_>,
        request_context: &RequestContext,
        presented_session: &PresentedSession,
    ) -> Result<VerifiedSignIn, AuthError> {
        let active_challenge = self
            .find_active_auth_challenge
            .execute(self.db_pool, code_entry.challenge_token)
            .await?
            .filter(|found_challenge| found_challenge.purpose == AuthChallengePurpose::MfaVerify)
            .ok_or(AuthError::InvalidChallenge)?;
        let challenged_user = self
            .find_user_by_id
            .execute(self.db_pool, active_challenge.user_id)
            .await?
            .ok_or(AuthError::InvalidChallenge)?;
        let throttle_attempt = ThrottleAttempt {
            scope: ThrottleScope::CodeEntry,
            account_key: email_account_key(&challenged_user.email),
            client_ip: request_context.client_ip,
            has_valid_remembered_device: false,
        };
        self.check_attempt_allowed
            .execute(&throttle_attempt)
            .await?;

        let mut verify_transaction = self.db_pool.begin().await?;
        let claimed_challenge = self
            .claim_auth_challenge_attempt
            .execute(
                &mut *verify_transaction,
                code_entry.challenge_token,
                AuthChallengePurpose::MfaVerify,
            )
            .await?
            .ok_or(AuthError::InvalidChallenge)?;
        let Some(accepted_factor) = self
            .accept_second_factor
            .execute(
                &mut verify_transaction,
                challenged_user.id,
                code_entry.entered_code,
            )
            .await?
        else {
            if claimed_challenge.is_out_of_attempts() {
                self.delete_auth_challenge
                    .execute(&mut *verify_transaction, claimed_challenge.id)
                    .await?;
            }
            self.record(
                &mut verify_transaction,
                SecurityEventType::SignInFailed,
                &challenged_user,
                request_context,
                SecurityEventDetails::default().with_text("stage", "second_factor")?,
            )
            .await?;
            verify_transaction.commit().await?;
            self.record_failed_attempt
                .execute(&throttle_attempt)
                .await?;
            return Err(AuthError::WrongCode);
        };

        self.delete_auth_challenge
            .execute(&mut *verify_transaction, claimed_challenge.id)
            .await?;
        let session_tokens = self
            .create_session
            .execute(
                &mut verify_transaction,
                challenged_user.id,
                request_context,
                presented_session,
            )
            .await?;
        let device_token = if code_entry.should_remember_device {
            Some(
                self.remember_device
                    .execute(
                        &mut *verify_transaction,
                        challenged_user.id,
                        request_context.user_agent.as_deref(),
                    )
                    .await?,
            )
        } else {
            None
        };
        let remaining_recovery_codes = accepted_factor.remaining_recovery_codes();
        self.record(
            &mut verify_transaction,
            SecurityEventType::SignInSucceeded,
            &challenged_user,
            request_context,
            SecurityEventDetails::default()
                .with_text("method", accepted_factor.method_name())?
                .with_flag("remembered_device", code_entry.should_remember_device)?,
        )
        .await?;
        if let Some(remaining_count) = remaining_recovery_codes {
            self.record(
                &mut verify_transaction,
                SecurityEventType::RecoveryCodeUsed,
                &challenged_user,
                request_context,
                SecurityEventDetails::default().with_number("remaining_count", remaining_count)?,
            )
            .await?;
        }
        verify_transaction.commit().await?;
        self.clear_account_ip_throttle
            .execute(&throttle_attempt)
            .await?;
        Ok(VerifiedSignIn {
            session_tokens,
            device_token,
            remaining_recovery_codes,
        })
    }

    async fn record(
        &self,
        connection: &mut PgConnection,
        event_type: SecurityEventType,
        challenged_user: &User,
        request_context: &RequestContext,
        details: SecurityEventDetails,
    ) -> Result<(), AuthError> {
        self.record_security_event
            .execute(
                connection,
                &NewSecurityEvent {
                    event_type,
                    subject_ref: Some(challenged_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details,
                },
            )
            .await?;
        Ok(())
    }
}
