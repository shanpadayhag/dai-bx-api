use lettre::message::Mailbox;

use crate::core::mail_transport::MailTransport;
use crate::features::email::data_sources::smtp::smtp_mail_source::SmtpMailSource;
use crate::features::email::entities::account_email::AccountEmailKind;
use crate::features::email::error::EmailError;

pub struct SendAccountEmail<'a> {
    smtp_mail_source: SmtpMailSource<'a>,
    app_origin: &'a str,
}

impl<'a> SendAccountEmail<'a> {
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

    pub async fn execute(
        &self,
        email_kind: AccountEmailKind,
        recipient: &str,
        link_token: &str,
    ) -> Result<(), EmailError> {
        let action_link = format!(
            "{}{}?token={link_token}",
            self.app_origin.trim_end_matches('/'),
            email_kind.link_path(),
        );
        let outgoing_email = email_kind.compose(recipient, &action_link);
        self.smtp_mail_source.send(&outgoing_email).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::smtp_settings::SmtpSettings;
    use crate::core::mail_transport::build_mail_transport;
    use crate::features::email::mailpit_test_support::{
        mailpit_smtp_settings, read_message_sent_to, unique_test_recipient,
    };

    const APP_ORIGIN: &str = "https://dai-bx-web.vercel.app";
    const LINK_TOKEN: &str = "qL3n0Y7k_Wm2-Rx8VbT5cZpJ4hGd6sFa1eN9uKoXyQw";

    async fn send_and_read(email_kind: AccountEmailKind, app_origin: &str) -> (String, String) {
        let smtp_settings = mailpit_smtp_settings();
        let mail_transport = build_mail_transport(&smtp_settings);
        let recipient = unique_test_recipient();

        SendAccountEmail::new(&mail_transport, &smtp_settings.mail_from, app_origin)
            .execute(email_kind, &recipient, LINK_TOKEN)
            .await
            .expect("the email should send");

        let delivered_message = read_message_sent_to(&recipient).await;
        (delivered_message.subject, delivered_message.text_body)
    }

    #[tokio::test]
    async fn sends_a_verification_link_to_the_verify_email_page() {
        let (delivered_subject, delivered_text) =
            send_and_read(AccountEmailKind::VerifyEmail, APP_ORIGIN).await;

        assert_eq!(delivered_subject, "Confirm your email for DaiBX");
        assert!(delivered_text.contains(&format!("{APP_ORIGIN}/verify-email?token={LINK_TOKEN}")));
    }

    #[tokio::test]
    async fn sends_a_reset_link_to_the_reset_password_page() {
        let (_, delivered_text) = send_and_read(AccountEmailKind::PasswordReset, APP_ORIGIN).await;

        assert!(
            delivered_text.contains(&format!("{APP_ORIGIN}/reset-password?token={LINK_TOKEN}"))
        );
    }

    #[tokio::test]
    async fn sends_a_change_link_to_the_confirm_email_page() {
        let (_, delivered_text) =
            send_and_read(AccountEmailKind::ConfirmEmailChange, APP_ORIGIN).await;

        assert!(delivered_text.contains(&format!("{APP_ORIGIN}/confirm-email?token={LINK_TOKEN}")));
    }

    #[tokio::test]
    async fn ignores_a_trailing_slash_on_the_app_origin() {
        let (_, delivered_text) = send_and_read(
            AccountEmailKind::VerifyEmail,
            "https://dai-bx-web.vercel.app/",
        )
        .await;

        assert!(delivered_text.contains(&format!("{APP_ORIGIN}/verify-email?token=")));
    }

    #[tokio::test]
    async fn reports_a_failure_when_the_email_cannot_be_sent() {
        let unreachable_settings = SmtpSettings {
            port: 1,
            ..mailpit_smtp_settings()
        };
        let mail_transport = build_mail_transport(&unreachable_settings);

        let send_result =
            SendAccountEmail::new(&mail_transport, &unreachable_settings.mail_from, APP_ORIGIN)
                .execute(
                    AccountEmailKind::PasswordReset,
                    &unique_test_recipient(),
                    LINK_TOKEN,
                )
                .await;

        assert!(matches!(send_result, Err(EmailError::Send(_))));
    }
}
