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
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;
    use crate::core::config::smtp_settings::SmtpSettings;
    use crate::core::mail_transport::build_mail_transport;
    use crate::features::email::error::SmtpFailureKind;
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

    async fn serve_a_rejecting_smtp_server(rejection_line: &'static str) -> u16 {
        let smtp_listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let smtp_port = smtp_listener.local_addr().expect("the port").port();
        tokio::spawn(async move {
            let (smtp_stream, _) = smtp_listener.accept().await.expect("a connection");
            let (read_half, mut write_half) = smtp_stream.into_split();
            let mut client_lines = BufReader::new(read_half).lines();
            write_half
                .write_all(b"220 test ready\r\n")
                .await
                .expect("the greeting");
            while let Ok(Some(client_line)) = client_lines.next_line().await {
                let server_reply = if client_line.starts_with("RCPT") {
                    rejection_line
                } else if client_line.starts_with("QUIT") {
                    "221 bye\r\n"
                } else {
                    "250 ok\r\n"
                };
                write_half
                    .write_all(server_reply.as_bytes())
                    .await
                    .expect("the reply");
            }
        });
        smtp_port
    }

    #[tokio::test]
    async fn keeps_the_mail_servers_words_out_of_its_error() {
        let smtp_port =
            serve_a_rejecting_smtp_server("550 no mailbox for private.person@daibx.test\r\n").await;
        let rejecting_settings = SmtpSettings {
            port: smtp_port,
            ..mailpit_smtp_settings()
        };
        let mail_transport = build_mail_transport(&rejecting_settings);

        let send_error = SmtpMailSource::new(&mail_transport, &rejecting_settings.mail_from)
            .send(&outgoing_email_to("private.person@daibx.test"))
            .await
            .expect_err("the server refuses the recipient");

        assert!(matches!(
            send_error,
            EmailError::Send(SmtpFailureKind::Permanent)
        ));
        assert!(!send_error.to_string().contains("private.person"));
        assert!(!format!("{send_error:?}").contains("private.person"));
    }
}
