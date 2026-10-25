use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::{ConfirmIdentity, IdentityProof};
use crate::features::account::error::AccountError;
use crate::features::recovery_codes::actions::issue_recovery_codes::IssueRecoveryCodes;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::entities::current_session::CurrentSession;

pub struct RegenerateRecoveryCodes<'a> {
    db_pool: &'a PgPool,
    confirm_identity: ConfirmIdentity<'a>,
    issue_recovery_codes: IssueRecoveryCodes,
    record_security_event: RecordSecurityEvent,
}

impl<'a> RegenerateRecoveryCodes<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            confirm_identity: ConfirmIdentity::new(app_state),
            issue_recovery_codes: IssueRecoveryCodes::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        identity_proof: IdentityProof<'_>,
        request_context: &RequestContext,
    ) -> Result<Vec<Zeroizing<String>>, AccountError> {
        let mut regenerate_transaction = self.db_pool.begin().await?;
        let signed_in_user = self
            .confirm_identity
            .execute(
                &mut regenerate_transaction,
                current_session,
                identity_proof,
                request_context,
            )
            .await?;
        let recovery_codes = self
            .issue_recovery_codes
            .execute(&mut *regenerate_transaction, signed_in_user.id)
            .await?;
        self.record_security_event
            .execute(
                &mut *regenerate_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::RecoveryCodesRegenerated,
                    subject_ref: Some(signed_in_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        regenerate_transaction.commit().await?;
        Ok(recovery_codes)
    }
}
