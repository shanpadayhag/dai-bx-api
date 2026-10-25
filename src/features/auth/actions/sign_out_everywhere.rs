use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::remembered_devices::actions::forget_remembered_devices::ForgetRememberedDevices;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::sessions::actions::revoke_all_sessions::RevokeAllSessions;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::sessions::entities::session::RevokedReason;
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct SignOutEverywhere<'a> {
    db_pool: &'a PgPool,
    revoke_all_sessions: RevokeAllSessions,
    forget_remembered_devices: ForgetRememberedDevices,
    find_user_by_id: FindUserById,
    record_security_event: RecordSecurityEvent,
}

impl<'a> SignOutEverywhere<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            revoke_all_sessions: RevokeAllSessions::new(),
            forget_remembered_devices: ForgetRememberedDevices::new(),
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
        let ended_session_count = self
            .revoke_all_sessions
            .execute(
                &mut *sign_out_transaction,
                current_session.user_id,
                RevokedReason::SignedOutEverywhere,
            )
            .await?;
        let forgotten_device_count = self
            .forget_remembered_devices
            .execute(&mut *sign_out_transaction, current_session.user_id)
            .await?;
        let signed_out_user = self
            .find_user_by_id
            .execute(&mut *sign_out_transaction, current_session.user_id)
            .await?;
        self.record_security_event
            .execute(
                &mut *sign_out_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::SignedOutEverywhere,
                    subject_ref: signed_out_user.map(|found_user| found_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_number("ended_session_count", count_as_i64(ended_session_count))?
                        .with_number(
                            "forgotten_device_count",
                            count_as_i64(forgotten_device_count),
                        )?,
                },
            )
            .await?;
        sign_out_transaction.commit().await?;
        Ok(())
    }
}

fn count_as_i64(row_count: u64) -> i64 {
    i64::try_from(row_count).unwrap_or(i64::MAX)
}
