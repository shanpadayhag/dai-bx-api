use std::fmt;
use std::sync::Arc;

use data_encoding::BASE64URL_NOPAD;
use web_push_native::jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair};
use zeroize::Zeroizing;

const VAPID_PRIVATE_KEY_LENGTH: usize = 32;
const VAPID_SUBJECT_PREFIXES: [&str; 2] = ["https://", "mailto:"];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VapidSettingsError {
    #[error("environment variable {0} must be set")]
    Missing(&'static str),
    #[error("environment variable {0} is not valid")]
    Invalid(&'static str),
}

#[derive(Clone)]
pub struct VapidSettings {
    pub key_pair: Arc<ES256KeyPair>,
    pub subject: String,
    pub application_server_key: String,
}

impl fmt::Debug for VapidSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VapidSettings")
            .field("subject", &self.subject)
            .field("application_server_key", &self.application_server_key)
            .finish_non_exhaustive()
    }
}

impl VapidSettings {
    pub fn from_variables(
        read_variable: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, VapidSettingsError> {
        let read_present = |variable_name: &'static str| {
            read_variable(variable_name)
                .map(|variable_value| variable_value.trim().to_owned())
                .filter(|variable_value| !variable_value.is_empty())
                .ok_or(VapidSettingsError::Missing(variable_name))
        };
        let private_key_text = Zeroizing::new(read_present("VAPID_PRIVATE_KEY")?);
        let subject = read_present("VAPID_SUBJECT")?;
        if !VAPID_SUBJECT_PREFIXES
            .iter()
            .any(|subject_prefix| subject.starts_with(subject_prefix))
        {
            return Err(VapidSettingsError::Invalid("VAPID_SUBJECT"));
        }
        Self::from_private_key(&private_key_text, subject)
    }

    pub fn from_private_key(
        private_key_text: &str,
        subject: String,
    ) -> Result<Self, VapidSettingsError> {
        let private_key_bytes = Zeroizing::new(
            BASE64URL_NOPAD
                .decode(private_key_text.trim_end_matches('=').as_bytes())
                .map_err(|_| VapidSettingsError::Invalid("VAPID_PRIVATE_KEY"))?,
        );
        if private_key_bytes.len() != VAPID_PRIVATE_KEY_LENGTH {
            return Err(VapidSettingsError::Invalid("VAPID_PRIVATE_KEY"));
        }
        let key_pair = ES256KeyPair::from_bytes(&private_key_bytes)
            .map_err(|_| VapidSettingsError::Invalid("VAPID_PRIVATE_KEY"))?;
        let application_server_key =
            BASE64URL_NOPAD.encode(&key_pair.public_key().public_key().to_bytes_uncompressed());
        Ok(Self {
            key_pair: Arc::new(key_pair),
            subject,
            application_server_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PRIVATE_KEY: &str = "RS0WdYWWo1HajXg3NZR1olzCf31i-ZBGDkFyCs7j1jw";

    fn read_from<'a>(
        variable_pairs: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |variable_name| {
            variable_pairs
                .iter()
                .find(|(pair_name, _)| *pair_name == variable_name)
                .map(|(_, pair_value)| (*pair_value).to_owned())
        }
    }

    #[test]
    fn derives_the_application_server_key_from_the_private_key() {
        let vapid_settings = VapidSettings::from_variables(read_from(&[
            ("VAPID_PRIVATE_KEY", TEST_PRIVATE_KEY),
            ("VAPID_SUBJECT", "https://dai-bx.vercel.app"),
        ]))
        .expect("valid settings should parse");

        let decoded_public_key = BASE64URL_NOPAD
            .decode(vapid_settings.application_server_key.as_bytes())
            .expect("the public key should be base64url");
        assert_eq!(decoded_public_key.len(), 65);
        assert_eq!(decoded_public_key[0], 0x04);
    }

    #[test]
    fn reports_each_missing_variable() {
        assert_eq!(
            VapidSettings::from_variables(read_from(&[(
                "VAPID_SUBJECT",
                "https://dai-bx.vercel.app"
            )]))
            .err(),
            Some(VapidSettingsError::Missing("VAPID_PRIVATE_KEY"))
        );
        assert_eq!(
            VapidSettings::from_variables(read_from(&[("VAPID_PRIVATE_KEY", TEST_PRIVATE_KEY)]))
                .err(),
            Some(VapidSettingsError::Missing("VAPID_SUBJECT"))
        );
    }

    #[test]
    fn rejects_a_short_key_and_a_subject_that_is_not_a_url_or_mailto() {
        assert_eq!(
            VapidSettings::from_variables(read_from(&[
                ("VAPID_PRIVATE_KEY", "c2hvcnQ"),
                ("VAPID_SUBJECT", "https://dai-bx.vercel.app"),
            ]))
            .err(),
            Some(VapidSettingsError::Invalid("VAPID_PRIVATE_KEY"))
        );
        assert_eq!(
            VapidSettings::from_variables(read_from(&[
                ("VAPID_PRIVATE_KEY", TEST_PRIVATE_KEY),
                ("VAPID_SUBJECT", "dai-bx.vercel.app"),
            ]))
            .err(),
            Some(VapidSettingsError::Invalid("VAPID_SUBJECT"))
        );
    }

    #[test]
    fn never_prints_the_private_key() {
        let vapid_settings = VapidSettings::from_private_key(
            TEST_PRIVATE_KEY,
            String::from("https://dai-bx.vercel.app"),
        )
        .expect("valid settings should parse");

        assert!(!format!("{vapid_settings:?}").contains(TEST_PRIVATE_KEY));
    }
}
