use sqlx::PgPool;
use zeroize::Zeroizing;

use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::account::actions::confirm_identity::{ConfirmIdentity, IdentityProof};
use crate::features::account::error::AccountError;
use crate::features::email::actions::send_account_email::SendAccountEmail;
use crate::features::email::entities::account_email::AccountEmailKind;
use crate::features::email_tokens::actions::issue_email_token::IssueEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::throttling::actions::allow_email_send::AllowEmailSend;
use crate::features::throttling::entities::throttle_attempt::EmailSendPurpose;
use crate::features::users::actions::find_user_by_email::FindUserByEmail;

pub struct EmailChangeRequest<'a> {
    pub password: Zeroizing<String>,
    pub entered_code: &'a str,
    pub new_email: &'a str,
}

pub struct RequestEmailChange<'a> {
    db_pool: &'a PgPool,
    confirm_identity: ConfirmIdentity<'a>,
    find_user_by_email: FindUserByEmail<'a>,
    allow_email_send: AllowEmailSend<'a>,
    issue_email_token: IssueEmailToken,
    send_account_email: SendAccountEmail<'a>,
}

impl<'a> RequestEmailChange<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        let db_pool = &app_state.db_pool;
        let app_config = &app_state.app_config;
        Self {
            db_pool,
            confirm_identity: ConfirmIdentity::new(app_state),
            find_user_by_email: FindUserByEmail::new(db_pool),
            allow_email_send: AllowEmailSend::new(db_pool),
            issue_email_token: IssueEmailToken::new(),
            send_account_email: SendAccountEmail::new(
                &app_state.mail_transport,
                &app_config.smtp_settings.mail_from,
                &app_config.app_origin,
            ),
        }
    }

    pub async fn execute(
        &self,
        current_session: CurrentSession,
        email_change_request: EmailChangeRequest<'_>,
        request_context: &RequestContext,
    ) -> Result<(), AccountError> {
        let mut request_transaction = self.db_pool.begin().await?;
        let signed_in_user = self
            .confirm_identity
            .execute(
                &mut request_transaction,
                current_session,
                IdentityProof {
                    password: email_change_request.password,
                    entered_code: email_change_request.entered_code,
                },
                request_context,
            )
            .await?;
        if signed_in_user
            .email
            .eq_ignore_ascii_case(email_change_request.new_email)
        {
            return Err(AccountError::SameEmail);
        }
        self.allow_email_send
            .execute(
                EmailSendPurpose::ChangeEmail,
                email_change_request.new_email,
            )
            .await?;
        let is_address_taken = self
            .find_user_by_email
            .execute(email_change_request.new_email)
            .await?
            .is_some();
        if !is_address_taken {
            let link_token = self
                .issue_email_token
                .execute(
                    &mut request_transaction,
                    signed_in_user.id,
                    EmailTokenPurpose::ChangeEmail,
                    Some(email_change_request.new_email),
                )
                .await?;
            self.send_account_email
                .execute(
                    AccountEmailKind::ConfirmEmailChange,
                    email_change_request.new_email,
                    &link_token,
                )
                .await?;
        }
        request_transaction.commit().await?;
        Ok(())
    }
}
