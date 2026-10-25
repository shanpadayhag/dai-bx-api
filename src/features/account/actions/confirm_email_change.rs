use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::data_sources::postgres::account_source::PostgresAccountSource;
use crate::features::account::error::AccountError;
use crate::features::email_tokens::actions::consume_email_token::ConsumeEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::users::actions::find_user_by_id::FindUserById;

pub struct ConfirmEmailChange<'a> {
    db_pool: &'a PgPool,
    consume_email_token: ConsumeEmailToken,
    postgres_account_source: PostgresAccountSource,
    find_user_by_id: FindUserById,
    record_security_event: RecordSecurityEvent,
}

impl<'a> ConfirmEmailChange<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            consume_email_token: ConsumeEmailToken::new(),
            postgres_account_source: PostgresAccountSource,
            find_user_by_id: FindUserById::new(),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        link_token: &str,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        let mut confirm_transaction = self.db_pool.begin().await?;
        let consumed_token = self
            .consume_email_token
            .execute(
                &mut *confirm_transaction,
                link_token,
                EmailTokenPurpose::ChangeEmail,
            )
            .await?
            .ok_or(AccountError::InvalidLink)?;
        let new_email = consumed_token
            .new_email
            .as_deref()
            .ok_or(AccountError::InvalidLink)?;
        let is_changed = self
            .postgres_account_source
            .change_email(&mut *confirm_transaction, consumed_token.user_id, new_email)
            .await?;
        if !is_changed {
            return Err(AccountError::EmailTaken);
        }
        let changed_user = self
            .find_user_by_id
            .execute(&mut *confirm_transaction, consumed_token.user_id)
            .await?
            .ok_or(AccountError::InvalidLink)?;
        self.record_security_event
            .execute(
                &mut *confirm_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::EmailChanged,
                    subject_ref: Some(changed_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default(),
                },
            )
            .await?;
        confirm_transaction.commit().await?;
        Ok(())
    }
}
