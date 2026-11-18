use lettre::message::Mailbox;

use crate::core::mail_transport::MailTransport;
use crate::features::email::data_sources::smtp::smtp_mail_source::SmtpMailSource;
use crate::features::email::entities::sign_in_alert_email::{
    FORGOT_PASSWORD_PATH, SignInAlertEmail,
};
use crate::features::email::error::EmailError;

pub struct SendSignInAlertEmail<'a> {
    smtp_mail_source: SmtpMailSource<'a>,
    app_origin: &'a str,
}

impl<'a> SendSignInAlertEmail<'a> {
    pub fn new(
        mail_transport: &'a MailTransport,
        mail_from: &'a Mailbox,
        app_origin: &'a str,
    ) -> Self {
        Self {
            smtp_mail_source: SmtpMailSource::new(mail_transport, mail_from),
            app_origin,
        }
    }

    pub async fn execute(&self, alert_email: &SignInAlertEmail<'_>) -> Result<(), EmailError> {
        let forgot_password_link = format!(
            "{}{FORGOT_PASSWORD_PATH}",
            self.app_origin.trim_end_matches('/'),
        );
        self.smtp_mail_source
            .send(&alert_email.compose(&forgot_password_link))
            .await
    }
}
