use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::security::password_hash::hash_password;
use crate::core::state::AppState;
use crate::features::account::actions::check_current_password::CheckCurrentPassword;
use crate::features::account::error::AccountError;
use crate::features::password_policy::actions::check_new_password::CheckNewPassword;
use crate::features::password_policy::entities::new_password::PasswordContext;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::revoke_other_sessions::RevokeOtherSessions;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;
use crate::features::users::actions::update_password_hash::UpdatePasswordHash;

pub struct PasswordChange {
    pub current_password: Zeroizing<String>,
    pub new_password: Zeroizing<String>,
}

pub struct ChangePassword<'a> {
    db_pool: &'a PgPool,
    find_user_by_id: FindUserById,
    check_current_password: CheckCurrentPassword<'a>,
    check_new_password: CheckNewPassword<'a>,
    update_password_hash: UpdatePasswordHash,
    revoke_other_sessions: RevokeOtherSessions,
    record_security_event: RecordSecurityEvent,
}

impl<'a> ChangePassword<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            find_user_by_id: FindUserById::new(),
            check_current_password: CheckCurrentPassword::new(&app_state.db_pool),
            check_new_password: CheckNewPassword::new(
                &app_state.http_client,
                &app_state.app_config.hibp_api_base_url,
            ),
            update_password_hash: UpdatePasswordHash::new(),
            revoke_other_sessions: RevokeOtherSessions::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        password_change: PasswordChange,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        let signed_in_user = self
            .find_user_by_id
            .execute(self.db_pool, current_session.user_id)
            .await?
            .ok_or(AccountError::NotSignedIn)?;
        let is_current_password = self
            .check_current_password
            .execute(
                &signed_in_user,
                password_change.current_password,
                request_context,
            )
            .await?;
        if !is_current_password {
            return Err(AccountError::WrongCurrentPassword);
        }
        self.check_new_password
            .execute(
                &password_change.new_password,
                PasswordContext {
                    person_name: &signed_in_user.name,
                    email: &signed_in_user.email,
                },
            )
            .await?;
        let new_password_hash = hash_password(password_change.new_password).await?;

        let mut change_transaction = self.db_pool.begin().await?;
        self.update_password_hash
            .execute(
                &mut *change_transaction,
                signed_in_user.id,
                &new_password_hash,
            )
            .await?;
        let ended_session_count = self
            .revoke_other_sessions
            .execute(
                &mut *change_transaction,
                current_session,
                RevokedReason::PasswordChanged,
            )
            .await?;
        self.record_security_event
            .execute(
                &mut *change_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::PasswordChanged,
                    subject_ref: Some(signed_in_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default().with_number(
                        "ended_session_count",
                        i64::try_from(ended_session_count).unwrap_or(i64::MAX),
                    )?,
                },
            )
            .await?;
        change_transaction.commit().await?;
        Ok(())
    }
}
