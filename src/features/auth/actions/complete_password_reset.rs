use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::security::password_hash::hash_password;
use crate::core::state::AppState;
use crate::features::auth::actions::challenge_after_password::ChallengeAfterPassword;
use crate::features::auth::actions::sign_in::SignInOutcome;
use crate::features::auth::error::AuthError;
use crate::features::email_tokens::actions::consume_email_token::ConsumeEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::password_policy::actions::check_new_password::CheckNewPassword;
use crate::features::password_policy::entities::new_password::PasswordContext;
use crate::features::remembered_devices::actions::forget_remembered_devices::ForgetRememberedDevices;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::revoke_all_sessions::RevokeAllSessions;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::actions::update_password_hash::UpdatePasswordHash;

pub struct CompletePasswordReset<'a> {
    db_pool: &'a PgPool,
    consume_email_token: ConsumeEmailToken,
    find_user_by_id: FindUserById,
    check_new_password: CheckNewPassword<'a>,
    update_password_hash: UpdatePasswordHash,
    revoke_all_sessions: RevokeAllSessions,
    forget_remembered_devices: ForgetRememberedDevices,
    record_security_event: RecordSecurityEvent,
    challenge_after_password: ChallengeAfterPassword<'a>,
}

impl<'a> CompletePasswordReset<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let app_config = &app_state.app_config;
        Self {
            db_pool: &app_state.db_pool,
            consume_email_token: ConsumeEmailToken::new(),
            find_user_by_id: FindUserById::new(),
            check_new_password: CheckNewPassword::new(
                &app_state.http_client,
                &app_config.hibp_api_base_url,
            ),
            update_password_hash: UpdatePasswordHash::new(),
            revoke_all_sessions: RevokeAllSessions::new(),
            forget_remembered_devices: ForgetRememberedDevices::new(),
            record_security_event: RecordSecurityEvent::new(),
            challenge_after_password: ChallengeAfterPassword::new(&app_config.totp_keyring),
        }
    }

    pub async fn execute(
        &self,
        link_token: &str,
        new_password: Zeroizing<String>,
        request_context: &RequestContext,
    ) -> Result<SignInOutcome, AuthError> {
        let mut reset_transaction = self.db_pool.begin().await?;
        let consumed_token = self
            .consume_email_token
            .execute(
                &mut *reset_transaction,
                link_token,
                EmailTokenPurpose::PasswordReset,
            )
            .await?
            .ok_or(AuthError::InvalidLink)?;
        let reset_user = self
            .find_user_by_id
            .execute(&mut *reset_transaction, consumed_token.user_id)
            .await?
            .ok_or(AuthError::InvalidLink)?;
        self.check_new_password
            .execute(
                &new_password,
                PasswordContext {
                    person_name: &reset_user.name,
                    email: &reset_user.email,
                },
            )
            .await?;
        let new_password_hash = hash_password(new_password).await?;
        self.update_password_hash
            .execute(&mut *reset_transaction, reset_user.id, &new_password_hash)
            .await?;
        let ended_session_count = self
            .revoke_all_sessions
            .execute(
                &mut *reset_transaction,
                reset_user.id,
                RevokedReason::PasswordReset,
            )
            .await?;
        let forgotten_device_count = self
            .forget_remembered_devices
            .execute(&mut *reset_transaction, reset_user.id)
            .await?;
        self.record_security_event
            .execute(
                &mut *reset_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::PasswordResetCompleted,
                    subject_ref: Some(reset_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
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
        let sign_in_outcome = self
            .challenge_after_password
            .execute(&mut *reset_transaction, &reset_user)
            .await?;
        reset_transaction.commit().await?;
        Ok(sign_in_outcome)
    }
}
