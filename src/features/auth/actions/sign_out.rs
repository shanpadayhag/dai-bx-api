use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::revoke_session::RevokeSession;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct SignOut<'a> {
    db_pool: &'a PgPool,
    revoke_session: RevokeSession,
    find_user_by_id: FindUserById,
    record_security_event: RecordSecurityEvent,
}

impl<'a> SignOut<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            revoke_session: RevokeSession::new(),
            find_user_by_id: FindUserById::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        request_context: &RequestContext,
    ) -> Result<(), AuthError> {
        let mut sign_out_transaction = self.db_pool.begin().await?;
        self.revoke_session
            .execute(
                &mut *sign_out_transaction,
                current_session.session_id,
                RevokedReason::SignedOut,
            )
            .await?;
        let signed_out_user = self
            .find_user_by_id
            .execute(&mut *sign_out_transaction, current_session.user_id)
            .await?;
        self.record_security_event
            .execute(
                &mut *sign_out_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::SessionRevoked,
                    subject_ref: signed_out_user.map(|found_user| found_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_text("reason", RevokedReason::SignedOut.as_str())?,
                },
            )
            .await?;
        sign_out_transaction.commit().await?;
        Ok(())
    }
}
