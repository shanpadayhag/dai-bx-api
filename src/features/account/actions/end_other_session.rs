use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::check_current_password::CheckCurrentPassword;
use crate::features::account::error::AccountError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::end_owned_session::EndOwnedSession;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct EndOtherSession<'a> {
    db_pool: &'a PgPool,
    find_user_by_id: FindUserById,
    check_current_password: CheckCurrentPassword<'a>,
    end_owned_session: EndOwnedSession,
    record_security_event: RecordSecurityEvent,
}

impl<'a> EndOtherSession<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            find_user_by_id: FindUserById::new(),
            check_current_password: CheckCurrentPassword::new(&app_state.db_pool),
            end_owned_session: EndOwnedSession::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        ended_session_id: Uuid,
        password: Zeroizing<String>,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        if ended_session_id == current_session.session_id {
            return Err(AccountError::CurrentSession);
        }
        let signed_in_user = self
            .find_user_by_id
            .execute(self.db_pool, current_session.user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)?;
        let is_current_password = self
            .check_current_password
            .execute(&signed_in_user, password, request_context)
            .await?;
        if !is_current_password {
            return Err(AccountError::WrongPassword);
        }
        let mut end_transaction = self.db_pool.begin().await?;
        let is_ended = self
            .end_owned_session
            .execute(
                &mut *end_transaction,
                ended_session_id,
                signed_in_user.id,
                RevokedReason::EndedFromSettings,
            )
            .await?;
        if !is_ended {
            return Err(AccountError::SessionAlreadyEnded);
        }
        self.record_security_event
            .execute(
                &mut *end_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::SessionRevoked,
                    subject_ref: Some(signed_in_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_text("reason", RevokedReason::EndedFromSettings.as_str())?,
                },
            )
            .await?;
        end_transaction.commit().await?;
        Ok(())
    }
}
