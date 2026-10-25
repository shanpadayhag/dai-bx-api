use std::time::Duration;

use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, Tokio1Executor};

use crate::core::config::smtp_settings::{SmtpSecurity, SmtpSettings};

pub type MailTransport = AsyncSmtpTransport<Tokio1Executor>;

const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

pub fn build_mail_transport(smtp_settings: &SmtpSettings) -> MailTransport {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let transport_builder = match smtp_settings.security {
        SmtpSecurity::StartTls => MailTransport::starttls_relay(&smtp_settings.host)
            .expect("the SMTP host must be a valid TLS server name"),
        SmtpSecurity::Plain => MailTransport::builder_dangerous(&smtp_settings.host),
    }
    .port(smtp_settings.port)
    .timeout(Some(SMTP_TIMEOUT));
    match &smtp_settings.credentials {
        Some(smtp_credentials) => transport_builder
            .credentials(Credentials::new(
                smtp_credentials.username.clone(),
                smtp_credentials.password.to_string(),
            ))
            .build(),
        None => transport_builder.build(),
    }
}
