use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::{ConfirmIdentity, IdentityProof};
use crate::features::account::entities::account_profile::ACCOUNT_DELETION_CONFIRMATION;
use crate::features::account::error::AccountError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::users::actions::delete_user::DeleteUser;

pub struct DeleteAccount<'a> {
    db_pool: &'a PgPool,
    confirm_identity: ConfirmIdentity<'a>,
    delete_user: DeleteUser,
    record_security_event: RecordSecurityEvent,
}

impl<'a> DeleteAccount<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            confirm_identity: ConfirmIdentity::new(app_state),
            delete_user: DeleteUser::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        identity_proof: IdentityProof<'_>,
        confirmation_text: &str,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        if confirmation_text.trim() != ACCOUNT_DELETION_CONFIRMATION {
            return Err(AccountError::WrongConfirmation);
        }
        let mut delete_transaction = self.db_pool.begin().await?;
        let deleted_user = self
            .confirm_identity
            .execute(
                &mut delete_transaction,
                current_session,
                identity_proof,
                request_context,
            )
            .await?;
        self.record_security_event
            .execute(
                &mut *delete_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::AccountDeleted,
                    subject_ref: Some(deleted_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        self.delete_user
            .execute(&mut *delete_transaction, deleted_user.id)
            .await?;
        delete_transaction.commit().await?;
        Ok(())
    }
}
