use lettre::message::{Mailbox, MultiPart};
use lettre::{AsyncTransport, Message};

use crate::core::mail_transport::MailTransport;
use crate::features::email::entities::outgoing_email::OutgoingEmail;
use crate::features::email::error::EmailError;

pub struct SmtpMailSource<'a> {
    mail_transport: &'a MailTransport,
    mail_from: &'a Mailbox,
}

impl<'a> SmtpMailSource<'a> {
    pub fn new(mail_transport: &'a MailTransport, mail_from: &'a Mailbox) -> Self {
        Self {
            mail_transport,
            mail_from,
        }
    }

    pub async fn send(&self, outgoing_email: &OutgoingEmail) -> Result<(), EmailError> {
        let recipient_mailbox: Mailbox = outgoing_email
            .recipient
            .parse()
            .map_err(|_| EmailError::InvalidRecipient)?;
        let email_message = Message::builder()
            .from(self.mail_from.clone())
            .to(recipient_mailbox)
            .subject(&outgoing_email.subject)
            .multipart(MultiPart::alternative_plain_html(
                outgoing_email.text_body.clone(),
                outgoing_email.html_body.clone(),
            ))?;
        self.mail_transport.send(email_message).await?;
        Ok(())
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

    fn outgoing_email_to(recipient: &str) -> OutgoingEmail {
        OutgoingEmail {
            recipient: recipient.to_owned(),
            subject: String::from("Test subject ✓"),
            text_body: String::from("Plain text body"),
            html_body: String::from("<p>HTML body</p>"),
        }
    }

    #[tokio::test]
    async fn delivers_the_subject_both_bodies_and_the_sender() {
        let smtp_settings = mailpit_smtp_settings();
        let mail_transport = build_mail_transport(&smtp_settings);
        let recipient = unique_test_recipient();

        SmtpMailSource::new(&mail_transport, &smtp_settings.mail_from)
            .send(&outgoing_email_to(&recipient))
            .await
            .expect("Mailpit should accept the email");

        let delivered_message = read_message_sent_to(&recipient).await;
        assert_eq!(delivered_message.subject, "Test subject ✓");
        assert_eq!(delivered_message.text_body.trim(), "Plain text body");
        assert!(delivered_message.html_body.contains("<p>HTML body</p>"));
        assert_eq!(delivered_message.sender.address, "no-reply@daibx.test");
        assert_eq!(delivered_message.sender.name, "DaiBX");
    }

    #[tokio::test]
    #[ignore = "sends a real email; run with SMTP_* settings and TEST_EMAIL_RECIPIENT set"]
    async fn sends_a_real_email_through_the_configured_smtp_server() {
        let smtp_settings =
            SmtpSettings::from_variables(|variable_name| std::env::var(variable_name).ok())
                .expect("SMTP settings should be in the environment");
        let test_recipient = std::env::var("TEST_EMAIL_RECIPIENT")
            .expect("TEST_EMAIL_RECIPIENT should name your inbox");
        let mail_transport = build_mail_transport(&smtp_settings);

        SmtpMailSource::new(&mail_transport, &smtp_settings.mail_from)
            .send(&OutgoingEmail {
                recipient: test_recipient,
                subject: String::from("DaiBX test email"),
                text_body: String::from("If this reached your inbox, DaiBX can send email."),
                html_body: String::from("<p>If this reached your inbox, DaiBX can send email.</p>"),
            })
            .await
            .expect("the SMTP server should accept the email");
    }

    #[tokio::test]
    async fn refuses_an_invalid_recipient_without_sending() {
        let smtp_settings = mailpit_smtp_settings();
        let mail_transport = build_mail_transport(&smtp_settings);

        let send_result = SmtpMailSource::new(&mail_transport, &smtp_settings.mail_from)
            .send(&outgoing_email_to("not an address"))
            .await;

        assert!(matches!(send_result, Err(EmailError::InvalidRecipient)));
    }

    #[tokio::test]
    async fn fails_when_the_smtp_server_is_unreachable() {
        let unreachable_settings = SmtpSettings {
            port: 1,
            ..mailpit_smtp_settings()
        };
        let mail_transport = build_mail_transport(&unreachable_settings);

        let send_result = SmtpMailSource::new(&mail_transport, &unreachable_settings.mail_from)
            .send(&outgoing_email_to(&unique_test_recipient()))
            .await;

        assert!(matches!(send_result, Err(EmailError::Send(_))));
    }
}
