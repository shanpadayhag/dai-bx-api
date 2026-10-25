use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::actions::create_auth_challenge::CreateAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::email_tokens::actions::consume_email_token::ConsumeEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::actions::mark_email_verified::MarkEmailVerified;

pub struct VerifyEmail<'a> {
    db_pool: &'a PgPool,
    consume_email_token: ConsumeEmailToken,
    find_user_by_id: FindUserById,
    mark_email_verified: MarkEmailVerified,
    record_security_event: RecordSecurityEvent,
    create_auth_challenge: CreateAuthChallenge<'a>,
}

impl<'a> VerifyEmail<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            consume_email_token: ConsumeEmailToken::new(),
            find_user_by_id: FindUserById::new(),
            mark_email_verified: MarkEmailVerified::new(),
            record_security_event: RecordSecurityEvent::new(),
            create_auth_challenge: CreateAuthChallenge::new(&app_state.app_config.totp_keyring),
        }
    }

    pub async fn execute(
        &self,
        link_token: &str,
        request_context: &RequestContext,
    ) -> Result<Zeroizing<String>, AuthError> {
        let mut verify_transaction = self.db_pool.begin().await?;
        let consumed_token = self
            .consume_email_token
            .execute(
                &mut *verify_transaction,
                link_token,
                EmailTokenPurpose::VerifyEmail,
            )
            .await?
            .ok_or(AuthError::InvalidLink)?;
        let verified_user = self
            .find_user_by_id
            .execute(&mut *verify_transaction, consumed_token.user_id)
            .await?
            .ok_or(AuthError::InvalidLink)?;
        self.mark_email_verified
            .execute(&mut *verify_transaction, verified_user.id)
            .await?;
        self.record_security_event
            .execute(
                &mut *verify_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::EmailVerified,
                    subject_ref: Some(verified_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        let challenge_token = self
            .create_auth_challenge
            .execute(
                &mut *verify_transaction,
                verified_user.id,
                AuthChallengePurpose::MfaSetup,
            )
            .await?;
        verify_transaction.commit().await?;
        Ok(challenge_token)
    }
}
