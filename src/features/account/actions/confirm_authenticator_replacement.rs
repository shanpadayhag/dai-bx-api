use sqlx::PgPool;

use crate::core::clock::current_unix_seconds;
use crate::core::request_context::RequestContext;
use crate::core::security::totp::{TOTP_DIGIT_COUNT, find_accepted_totp_step};
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::core::state::AppState;
use crate::features::account::error::AccountError;
use crate::features::auth_challenges::actions::claim_auth_challenge_attempt::ClaimAuthChallengeAttempt;
use crate::features::auth_challenges::actions::delete_auth_challenge::DeleteAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::remembered_devices::actions::forget_remembered_devices::ForgetRememberedDevices;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::revoke_other_sessions::RevokeOtherSessions;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::actions::replace_totp_secret::ReplaceTotpSecret;

pub struct ConfirmAuthenticatorReplacement<'a> {
    db_pool: &'a PgPool,
    totp_keyring: &'a TotpKeyring,
    claim_auth_challenge_attempt: ClaimAuthChallengeAttempt,
    delete_auth_challenge: DeleteAuthChallenge,
    find_user_by_id: FindUserById,
    replace_totp_secret: ReplaceTotpSecret,
    revoke_other_sessions: RevokeOtherSessions,
    forget_remembered_devices: ForgetRememberedDevices,
    record_security_event: RecordSecurityEvent,
}

impl<'a> ConfirmAuthenticatorReplacement<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            totp_keyring: &app_state.app_config.totp_keyring,
            claim_auth_challenge_attempt: ClaimAuthChallengeAttempt::new(),
            delete_auth_challenge: DeleteAuthChallenge::new(),
            find_user_by_id: FindUserById::new(),
            replace_totp_secret: ReplaceTotpSecret::new(),
            revoke_other_sessions: RevokeOtherSessions::new(),
            forget_remembered_devices: ForgetRememberedDevices::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        challenge_token: &str,
        entered_code: &str,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        let is_well_formed_code = entered_code.len() == TOTP_DIGIT_COUNT
            && entered_code
                .bytes()
                .all(|code_byte| code_byte.is_ascii_digit());
        if !is_well_formed_code {
            return Err(AccountError::WrongCode);
        }
        let mut confirm_transaction = self.db_pool.begin().await?;
        let claimed_challenge = self
            .claim_auth_challenge_attempt
            .execute(
                &mut *confirm_transaction,
                challenge_token,
                AuthChallengePurpose::MfaReplace,
            )
            .await?
            .filter(|found_challenge| found_challenge.user_id == current_session.user_id)
            .ok_or(AccountError::StepExpired)?;
        let sealed_totp_secret = claimed_challenge
            .pending_totp_secret
            .as_deref()
            .ok_or(AccountError::StepExpired)?;
        let pending_totp_secret = self
            .totp_keyring
            .open(claimed_challenge.user_id, sealed_totp_secret)?;
        let Some(accepted_totp_step) = find_accepted_totp_step(
            &pending_totp_secret,
            entered_code,
            current_unix_seconds(),
            None,
        ) else {
            if claimed_challenge.is_out_of_attempts() {
                self.delete_auth_challenge
                    .execute(&mut *confirm_transaction, claimed_challenge.id)
                    .await?;
            }
            confirm_transaction.commit().await?;
            return Err(AccountError::WrongCode);
        };

        self.delete_auth_challenge
            .execute(&mut *confirm_transaction, claimed_challenge.id)
            .await?;
        self.replace_totp_secret
            .execute(
                &mut *confirm_transaction,
                current_session.user_id,
                sealed_totp_secret,
                accepted_totp_step,
            )
            .await?;
        let ended_session_count = self
            .revoke_other_sessions
            .execute(
                &mut *confirm_transaction,
                current_session,
                RevokedReason::AuthenticatorReplaced,
            )
            .await?;
        let forgotten_device_count = self
            .forget_remembered_devices
            .execute(&mut *confirm_transaction, current_session.user_id)
            .await?;
        let signed_in_user = self
            .find_user_by_id
            .execute(&mut *confirm_transaction, current_session.user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)?;
        self.record_security_event
            .execute(
                &mut *confirm_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::MfaSetUp,
                    subject_ref: Some(signed_in_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_flag("replaced", true)?
                        .with_number(
                            "ended_session_count",
                            i64::try_from(ended_session_count).unwrap_or(i64::MAX),
                        )?
                        .with_number(
                            "forgotten_device_count",
                            i64::try_from(forgotten_device_count).unwrap_or(i64::MAX),
                        )?,
                },
            )
            .await?;
        confirm_transaction.commit().await?;
        Ok(())
    }
}
