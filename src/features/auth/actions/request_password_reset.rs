use sqlx::PgPool;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::email::actions::send_account_email::SendAccountEmail;
use crate::features::email::entities::account_email::AccountEmailKind;
use crate::features::email_tokens::actions::issue_email_token::IssueEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::security_log::actions::record_security_event::RecordSecurityEvent;
use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEventDetails, SecurityEventType,
};
use crate::features::throttling::actions::allow_email_send::AllowEmailSend;
use crate::features::throttling::entities::throttle_attempt::EmailSendPurpose;
use crate::features::users::actions::find_user_by_email::FindUserByEmail;

pub struct RequestPasswordReset<'a> {
    db_pool: &'a PgPool,
    allow_email_send: AllowEmailSend<'a>,
    find_user_by_email: FindUserByEmail<'a>,
    issue_email_token: IssueEmailToken,
    send_account_email: SendAccountEmail<'a>,
    record_security_event: RecordSecurityEvent,
}

impl<'a> RequestPasswordReset<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let app_config = &app_state.app_config;
        Self {
            db_pool: &app_state.db_pool,
            allow_email_send: AllowEmailSend::new(&app_state.db_pool),
            find_user_by_email: FindUserByEmail::new(&app_state.db_pool),
            issue_email_token: IssueEmailToken::new(),
            send_account_email: SendAccountEmail::new(
                &app_state.mail_transport,
                &app_config.smtp_settings.mail_from,
                &app_config.app_origin,
            ),
            record_security_event: RecordSecurityEvent::new(),
        }
    }

    pub async fn execute(
        &self,
        email: &str,
        request_context: &RequestContext,
    ) -> Result<(), AuthError> {
        self.allow_email_send
            .execute(EmailSendPurpose::PasswordReset, email)
            .await?;
        let found_user = self.find_user_by_email.execute(email).await?;
        let mut request_transaction = self.db_pool.begin().await?;
        self.record_security_event
            .execute(
                &mut *request_transaction,
                &NewSecurityEvent {
                    event_type: SecurityEventType::PasswordResetRequested,
                    subject_ref: found_user
                        .as_ref()
                        .map(|reset_user| reset_user.security_ref),
                    client_ip: request_context.client_ip,
                    user_agent: request_context.user_agent.clone(),
                    details: SecurityEventDetails::default()
                        .with_flag("account_found", found_user.is_some())?,
                },
            )
            .await?;
        if let Some(reset_user) = found_user {
            let link_token = self
                .issue_email_token
                .execute(
                    &mut request_transaction,
                    reset_user.id,
                    EmailTokenPurpose::PasswordReset,
                    None,
                )
                .await?;
            self.send_account_email
                .execute(
                    AccountEmailKind::PasswordReset,
                    &reset_user.email,
                    &link_token,
                )
                .await?;
        }
        request_transaction.commit().await?;
        Ok(())
    }
}
