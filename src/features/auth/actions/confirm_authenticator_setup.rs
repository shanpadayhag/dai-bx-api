use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::clock::current_unix_seconds;
use crate::core::request_context::RequestContext;
use crate::core::security::totp::{TOTP_DIGIT_COUNT, find_accepted_totp_step};
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::actions::claim_auth_challenge_attempt::ClaimAuthChallengeAttempt;
use crate::features::auth_challenges::actions::delete_auth_challenge::DeleteAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::recovery_codes::actions::issue_recovery_codes::IssueRecoveryCodes;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::create_session::CreateSession;
use crate::features::sessions::entities::presented_session::PresentedSession;
use crate::features::sessions::entities::session::IssuedSessionTokens;
use crate::features::users::actions::enable_mfa::EnableMfa;
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct ConfirmedAuthenticatorSetup {
    pub session_tokens: IssuedSessionTokens,
    pub recovery_codes: Vec<Zeroizing<String>>,
}

pub struct ConfirmAuthenticatorSetup<'a> {
    db_pool: &'a PgPool,
    totp_keyring: &'a TotpKeyring,
    claim_auth_challenge_attempt: ClaimAuthChallengeAttempt,
    delete_auth_challenge: DeleteAuthChallenge,
    enable_mfa: EnableMfa,
    find_user_by_id: FindUserById,
    issue_recovery_codes: IssueRecoveryCodes,
    create_session: CreateSession,
    record_security_event: RecordSecurityEvent,
}

impl<'a> ConfirmAuthenticatorSetup<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            totp_keyring: &app_state.app_config.totp_keyring,
            claim_auth_challenge_attempt: ClaimAuthChallengeAttempt::new(),
            delete_auth_challenge: DeleteAuthChallenge::new(),
            enable_mfa: EnableMfa::new(),
            find_user_by_id: FindUserById::new(),
            issue_recovery_codes: IssueRecoveryCodes::new(),
            create_session: CreateSession::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        challenge_token: &str,
        entered_code: &str,
        request_context: &RequestContext,
        presented_session: &PresentedSession,
    ) -> Result<ConfirmedAuthenticatorSetup, AuthError> {
        let is_well_formed_code = entered_code.len() == TOTP_DIGIT_COUNT
            && entered_code
                .bytes()
                .all(|code_byte| code_byte.is_ascii_digit());
        if !is_well_formed_code {
            return Err(AuthError::WrongCode);
        }
        let mut confirm_transaction = self.db_pool.begin().await?;
        let claimed_challenge = self
            .claim_auth_challenge_attempt
            .execute(
                &mut *confirm_transaction,
                challenge_token,
                AuthChallengePurpose::MfaSetup,
            )
            .await?
            .ok_or(AuthError::InvalidChallenge)?;
        let sealed_totp_secret = claimed_challenge
            .pending_totp_secret
            .as_deref()
            .ok_or(AuthError::InvalidChallenge)?;
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
            return Err(AuthError::WrongCode);
        };

        self.delete_auth_challenge
            .execute(&mut *confirm_transaction, claimed_challenge.id)
            .await?;
        let is_newly_enabled = self
            .enable_mfa
            .execute(
                &mut *confirm_transaction,
                claimed_challenge.user_id,
                sealed_totp_secret,
                accepted_totp_step,
            )
            .await?;
        if !is_newly_enabled {
            return Err(AuthError::InvalidChallenge);
        }
        let set_up_user = self
            .find_user_by_id
            .execute(&mut *confirm_transaction, claimed_challenge.user_id)
            .await?
            .ok_or(AuthError::InvalidChallenge)?;
        let recovery_codes = self
            .issue_recovery_codes
            .execute(&mut *confirm_transaction, set_up_user.id)
            .await?;
        let session_tokens = self
            .create_session
            .execute(
                &mut confirm_transaction,
                set_up_user.id,
                request_context,
                presented_session,
            )
            .await?;
        self.record_security_event
            .execute(
                &mut *confirm_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::MfaSetUp,
                    subject_ref: Some(set_up_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        confirm_transaction.commit().await?;
        Ok(ConfirmedAuthenticatorSetup {
            session_tokens,
            recovery_codes,
        })
    }
}
