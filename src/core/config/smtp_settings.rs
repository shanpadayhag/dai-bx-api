use std::fmt;

use lettre::message::Mailbox;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpSecurity {
    StartTls,
    Plain,
}

#[derive(Clone)]
pub struct SmtpCredentials {
    pub username: String,
    pub password: Zeroizing<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SmtpSettingsError {
    #[error("environment variable {0} must be set")]
    Missing(&'static str),
    #[error("environment variable {0} is not valid")]
    Invalid(&'static str),
}

#[derive(Debug, Clone)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    pub security: SmtpSecurity,
    pub credentials: Option<SmtpCredentials>,
    pub mail_from: Mailbox,
}

const DEFAULT_SMTP_PORT: u16 = 587;

impl SmtpSettings {
    pub fn from_variables(
        read_variable: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, SmtpSettingsError> {
        let read_present = |variable_name: &str| {
            read_variable(variable_name).filter(|variable_value| !variable_value.trim().is_empty())
        };
        let security = match read_present("SMTP_SECURITY").as_deref() {
            None | Some("starttls") => SmtpSecurity::StartTls,
            Some("plain") => SmtpSecurity::Plain,
            Some(_) => return Err(SmtpSettingsError::Invalid("SMTP_SECURITY")),
        };
        let port = match read_present("SMTP_PORT") {
            Some(port_text) => port_text
                .trim()
                .parse()
                .map_err(|_| SmtpSettingsError::Invalid("SMTP_PORT"))?,
            None => DEFAULT_SMTP_PORT,
        };
        let credentials = match (read_present("SMTP_USERNAME"), read_present("SMTP_PASSWORD")) {
            (Some(username), Some(password)) => Some(SmtpCredentials {
                username,
                password: Zeroizing::new(password),
            }),
            (None, None) => None,
            (Some(_), None) => return Err(SmtpSettingsError::Missing("SMTP_PASSWORD")),
            (None, Some(_)) => return Err(SmtpSettingsError::Missing("SMTP_USERNAME")),
        };
        Ok(Self {
            host: read_present("SMTP_HOST").ok_or(SmtpSettingsError::Missing("SMTP_HOST"))?,
            port,
            security,
            credentials,
            mail_from: read_present("MAIL_FROM")
                .ok_or(SmtpSettingsError::Missing("MAIL_FROM"))?
                .parse()
                .map_err(|_| SmtpSettingsError::Invalid("MAIL_FROM"))?,
        })
    }
}

impl fmt::Debug for SmtpCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SmtpCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_from(variable_pairs: &[(&str, &str)]) -> Result<SmtpSettings, SmtpSettingsError> {
        SmtpSettings::from_variables(|variable_name| {
            variable_pairs
                .iter()
                .find(|(pair_name, _)| *pair_name == variable_name)
                .map(|(_, pair_value)| (*pair_value).to_owned())
        })
    }

    #[test]
    fn reads_gmail_style_settings_with_starttls_on_587_by_default() {
        let smtp_settings = settings_from(&[
            ("SMTP_HOST", "smtp.gmail.com"),
            ("SMTP_USERNAME", "daibx.app@gmail.com"),
            ("SMTP_PASSWORD", "abcd efgh ijkl mnop"),
            ("MAIL_FROM", "DaiBX <daibx.app@gmail.com>"),
        ])
        .expect("settings should parse");

        assert_eq!(smtp_settings.port, 587);
        assert_eq!(smtp_settings.security, SmtpSecurity::StartTls);
        assert_eq!(
            smtp_settings.mail_from.email.to_string(),
            "daibx.app@gmail.com"
        );
        assert!(smtp_settings.credentials.is_some());
    }

    #[test]
    fn reads_local_mailpit_settings_without_credentials() {
        let smtp_settings = settings_from(&[
            ("SMTP_HOST", "127.0.0.1"),
            ("SMTP_PORT", "1025"),
            ("SMTP_SECURITY", "plain"),
            ("MAIL_FROM", "DaiBX <no-reply@daibx.test>"),
        ])
        .expect("settings should parse");

        assert_eq!(smtp_settings.port, 1025);
        assert_eq!(smtp_settings.security, SmtpSecurity::Plain);
        assert!(smtp_settings.credentials.is_none());
    }

    #[test]
    fn refuses_half_credentials_bad_values_and_missing_required_settings() {
        let base_pairs = [
            ("SMTP_HOST", "smtp.gmail.com"),
            ("MAIL_FROM", "DaiBX <a@b.co>"),
        ];

        assert!(matches!(
            settings_from(&[base_pairs[0], base_pairs[1], ("SMTP_USERNAME", "a@b.co")]),
            Err(SmtpSettingsError::Missing("SMTP_PASSWORD"))
        ));
        assert!(matches!(
            settings_from(&[base_pairs[0], base_pairs[1], ("SMTP_SECURITY", "tls")]),
            Err(SmtpSettingsError::Invalid("SMTP_SECURITY"))
        ));
        assert!(matches!(
            settings_from(&[base_pairs[0], base_pairs[1], ("SMTP_PORT", "smtp")]),
            Err(SmtpSettingsError::Invalid("SMTP_PORT"))
        ));
        assert!(matches!(
            settings_from(&[base_pairs[1]]),
            Err(SmtpSettingsError::Missing("SMTP_HOST"))
        ));
        assert!(matches!(
            settings_from(&[base_pairs[0], ("MAIL_FROM", "not an address")]),
            Err(SmtpSettingsError::Invalid("MAIL_FROM"))
        ));
    }

    #[test]
    fn never_prints_the_smtp_password_in_debug_output() {
        let smtp_credentials = SmtpCredentials {
            username: String::from("daibx.app@gmail.com"),
            password: Zeroizing::new(String::from("abcd efgh ijkl mnop")),
        };

        let debug_output = format!("{smtp_credentials:?}");

        assert!(!debug_output.contains("abcd efgh"));
        assert!(debug_output.contains("<redacted>"));
    }
}
