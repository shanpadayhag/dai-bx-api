use sqlx::{PgConnection, PgPool};
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::check_current_password::CheckCurrentPassword;
use crate::features::account::actions::check_second_factor::CheckSecondFactor;
use crate::features::account::error::AccountError;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::entities::user::User;

pub struct IdentityProof<'a> {
    pub password: Zeroizing<String>,
    pub entered_code: &'a str,
}

pub struct ConfirmIdentity<'a> {
    db_pool: &'a PgPool,
    find_user_by_id: FindUserById,
    check_current_password: CheckCurrentPassword<'a>,
    check_second_factor: CheckSecondFactor<'a>,
    record_security_event: RecordSecurityEvent,
}

impl<'a> ConfirmIdentity<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let db_pool = &app_state.db_pool;
        Self {
            db_pool,
            find_user_by_id: FindUserById::new(),
            check_current_password: CheckCurrentPassword::new(db_pool),
            check_second_factor: CheckSecondFactor::new(
                db_pool,
                &app_state.app_config.totp_keyring,
            ),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        current_session: CurrentSession,
        identity_proof: IdentityProof<'_>,
        request_context: &RequestContext,
    ) -> Result<User, AccountError> {
        let signed_in_user = self
            .find_user_by_id
            .execute(self.db_pool, current_session.user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)?;
        let is_current_password = self
            .check_current_password
            .execute(&signed_in_user, identity_proof.password, request_context)
            .await?;
        if !is_current_password {
            return Err(AccountError::WrongPassword);
        }
        let accepted_factor = self
            .check_second_factor
            .execute(
                &mut *connection,
                &signed_in_user,
                identity_proof.entered_code,
                request_context,
            )
            .await?;
        if let Some(remaining_count) = accepted_factor.remaining_recovery_codes() {
            self.record_security_event
                .execute(
                    &mut *connection,
                    &NewSecurityEvent {
                        event_type: SecurityEventType::RecoveryCodeUsed,
                        subject_ref: Some(signed_in_user.security_ref),
                        client_ip: request_context.client_ip,
                        user_agent: request_context.user_agent.clone(),
                        details: SecurityEventDetails::default()
                            .with_number("remaining_count", remaining_count)?,
                    },
                )
                .await?;
        }
        Ok(signed_in_user)
    }
}
