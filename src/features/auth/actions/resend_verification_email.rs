use sqlx::PgPool;

use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::email::actions::send_account_email::SendAccountEmail;
use crate::features::email::entities::account_email::AccountEmailKind;
use crate::features::email_tokens::actions::issue_email_token::IssueEmailToken;
use crate::features::email_tokens::entities::email_token::EmailTokenPurpose;
use crate::features::throttling::actions::allow_email_send::AllowEmailSend;
use crate::features::throttling::entities::throttle_attempt::EmailSendPurpose;
use crate::features::users::actions::find_user_by_email::FindUserByEmail;

pub struct ResendVerificationEmail<'a> {
    db_pool: &'a PgPool,
    allow_email_send: AllowEmailSend<'a>,
    find_user_by_email: FindUserByEmail<'a>,
    issue_email_token: IssueEmailToken,
    send_account_email: SendAccountEmail<'a>,
}

impl<'a> ResendVerificationEmail<'a> {
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
        }
    }

    pub async fn execute(&self, email: &str) -> Result<(), AuthError> {
        self.allow_email_send
            .execute(EmailSendPurpose::VerifyEmail, email)
            .await?;
        let Some(unverified_user) = self
            .find_user_by_email
            .execute(email)
            .await?
            .filter(|found_user| !found_user.is_email_verified)
        else {
            return Ok(());
        };
        let mut resend_transaction = self.db_pool.begin().await?;
        let link_token = self
            .issue_email_token
            .execute(
                &mut resend_transaction,
                unverified_user.id,
                EmailTokenPurpose::VerifyEmail,
                None,
            )
            .await?;
        self.send_account_email
            .execute(
                AccountEmailKind::VerifyEmail,
                &unverified_user.email,
                &link_token,
            )
            .await?;
        resend_transaction.commit().await?;
        Ok(())
    }
}
