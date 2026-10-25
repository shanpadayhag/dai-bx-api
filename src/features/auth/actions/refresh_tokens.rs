use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::renew_session::{RenewSession, SessionRenewal};
use crate::features::sessions::entities::session::IssuedSessionTokens;
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct RefreshTokens<'a> {
    db_pool: &'a PgPool,
    renew_session: RenewSession,
    find_user_by_id: FindUserById,
    record_security_event: RecordSecurityEvent,
}

impl<'a> RefreshTokens<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            renew_session: RenewSession::new(),
            find_user_by_id: FindUserById::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        refresh_token: &str,
        request_context: &RequestContext,
    ) -> Result<IssuedSessionTokens, AuthError> {
        let mut refresh_transaction = self.db_pool.begin().await?;
        match self
            .renew_session
            .execute(&mut refresh_transaction, refresh_token)
            .await?
        {
            SessionRenewal::Renewed { session_tokens } => {
                refresh_transaction.commit().await?;
                Ok(session_tokens)
            }
            SessionRenewal::ReuseDetected { user_id } => {
                let reused_by_user = self
                    .find_user_by_id
                    .execute(&mut *refresh_transaction, user_id)
                    .await?;
                self.record_security_event
                    .execute(
                        &mut *refresh_transaction,
                        &NewSecurityEvent {
                            event_type: SecurityEventType::RefreshTokenReuseDetected,
                            subject_ref: reused_by_user.map(|found_user| found_user.security_ref),
                            client_ip: request_context.client_ip,
                            user_agent: request_context.user_agent.clone(),
                            details: SecurityEventDetails::default(),
                        },
                    )
                    .await?;
                refresh_transaction.commit().await?;
                Err(AuthError::NotSignedIn)
            }
            SessionRenewal::Refused => Err(AuthError::NotSignedIn),
        }
    }
}
