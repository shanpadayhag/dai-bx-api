use std::collections::BTreeMap;
use std::fmt;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use data_encoding::BASE64;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::random_bytes::generate_random_bytes;

pub const TOTP_KEY_BYTE_COUNT: usize = 32;
const NONCE_BYTE_COUNT: usize = 12;

#[derive(Clone)]
pub struct TotpKeyring {
    keys_by_version: BTreeMap<u8, Zeroizing<[u8; TOTP_KEY_BYTE_COUNT]>>,
    current_key_version: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum TotpKeyringError {
    #[error("TOTP key entry {0:?} must look like v<1-255>:<base64 of 32 bytes>")]
    MalformedEntry(String),
    #[error("TOTP key version {0} appears more than once")]
    DuplicateVersion(u8),
    #[error("current TOTP key {0:?} is not in the keyring")]
    MissingCurrentKey(String),
}

#[derive(Debug, thiserror::Error)]
pub enum TotpCipherError {
    #[error("sealed TOTP secret is too short")]
    TooShort,
    #[error("sealed TOTP secret uses unknown key version {0}")]
    UnknownKeyVersion(u8),
    #[error("sealed TOTP secret failed authentication")]
    AuthenticationFailed,
}

impl fmt::Debug for TotpKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TotpKeyring")
            .field(
                "key_versions",
                &self.keys_by_version.keys().collect::<Vec<_>>(),
            )
            .field("current_key_version", &self.current_key_version)
            .finish()
    }
}

impl TotpKeyring {
    pub fn parse(keys_text: &str, current_key_label: &str) -> Result<Self, TotpKeyringError> {
        let mut keys_by_version = BTreeMap::new();
        for key_entry in keys_text
            .split(',')
            .map(str::trim)
            .filter(|key_entry| !key_entry.is_empty())
        {
            let (key_version, key_bytes) = parse_key_entry(key_entry)?;
            if keys_by_version.insert(key_version, key_bytes).is_some() {
                return Err(TotpKeyringError::DuplicateVersion(key_version));
            }
        }
        let current_key_version = parse_key_version(current_key_label.trim())
            .filter(|key_version| keys_by_version.contains_key(key_version))
            .ok_or_else(|| TotpKeyringError::MissingCurrentKey(current_key_label.to_owned()))?;
        Ok(Self {
            keys_by_version,
            current_key_version,
        })
    }

    pub fn seal(&self, user_id: Uuid, totp_secret: &[u8]) -> Vec<u8> {
        let nonce_bytes = generate_random_bytes::<NONCE_BYTE_COUNT>();
        let ciphertext = self
            .cipher_for(self.current_key_version)
            .expect("the current key is always in the keyring")
            .encrypt(
                &nonce_bytes.into(),
                Payload {
                    msg: totp_secret,
                    aad: user_id.as_bytes(),
                },
            )
            .expect("AES-GCM encryption of a short secret cannot fail");
        let mut sealed_secret = Vec::with_capacity(1 + NONCE_BYTE_COUNT + ciphertext.len());
        sealed_secret.push(self.current_key_version);
        sealed_secret.extend_from_slice(&nonce_bytes);
        sealed_secret.extend_from_slice(&ciphertext);
        sealed_secret
    }

    pub fn open(
        &self,
        user_id: Uuid,
        sealed_secret: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, TotpCipherError> {
        let (&key_version, nonce_and_ciphertext) = sealed_secret
            .split_first()
            .ok_or(TotpCipherError::TooShort)?;
        if nonce_and_ciphertext.len() <= NONCE_BYTE_COUNT {
            return Err(TotpCipherError::TooShort);
        }
        let (nonce_bytes, ciphertext) = nonce_and_ciphertext.split_at(NONCE_BYTE_COUNT);
        let nonce: [u8; NONCE_BYTE_COUNT] = nonce_bytes
            .try_into()
            .expect("split_at returns exactly the nonce length");
        self.cipher_for(key_version)
            .ok_or(TotpCipherError::UnknownKeyVersion(key_version))?
            .decrypt(
                &nonce.into(),
                Payload {
                    msg: ciphertext,
                    aad: user_id.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| TotpCipherError::AuthenticationFailed)
    }

    fn cipher_for(&self, key_version: u8) -> Option<Aes256Gcm> {
        self.keys_by_version.get(&key_version).map(|key_bytes| {
            Aes256Gcm::new_from_slice(key_bytes.as_slice()).expect("keys are always 32 bytes")
        })
    }
}

fn parse_key_entry(
    key_entry: &str,
) -> Result<(u8, Zeroizing<[u8; TOTP_KEY_BYTE_COUNT]>), TotpKeyringError> {
    let malformed_entry = || TotpKeyringError::MalformedEntry(redact_key_entry(key_entry));
    let (key_label, encoded_key) = key_entry.split_once(':').ok_or_else(malformed_entry)?;
    let key_version = parse_key_version(key_label).ok_or_else(malformed_entry)?;
    let decoded_key = Zeroizing::new(
        BASE64
            .decode(encoded_key.as_bytes())
            .map_err(|_| malformed_entry())?,
    );
    let key_bytes: [u8; TOTP_KEY_BYTE_COUNT] = decoded_key
        .as_slice()
        .try_into()
        .map_err(|_| malformed_entry())?;
    Ok((key_version, Zeroizing::new(key_bytes)))
}

fn parse_key_version(key_label: &str) -> Option<u8> {
    key_label
        .strip_prefix('v')
        .and_then(|version_digits| version_digits.parse::<u8>().ok())
        .filter(|key_version| *key_version > 0)
}

fn redact_key_entry(key_entry: &str) -> String {
    key_entry.split_once(':').map_or_else(
        || String::from("<unlabelled>"),
        |(key_label, _)| format!("{key_label}:<redacted>"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_KEY: &str = "v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const SECOND_KEY: &str = "v2:ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=";

    fn keyring(keys_text: &str, current_key_label: &str) -> TotpKeyring {
        TotpKeyring::parse(keys_text, current_key_label).expect("keyring should parse")
    }

    #[test]
    fn opens_what_it_sealed_for_the_same_user() {
        let totp_keyring = keyring(FIRST_KEY, "v1");
        let user_id = Uuid::now_v7();

        let sealed_secret = totp_keyring.seal(user_id, b"twenty byte secret!!");

        assert_eq!(sealed_secret[0], 1);
        assert_eq!(
            totp_keyring
                .open(user_id, &sealed_secret)
                .expect("should open")
                .as_slice(),
            b"twenty byte secret!!"
        );
    }

    #[test]
    fn refuses_to_open_a_secret_sealed_for_another_user() {
        let totp_keyring = keyring(FIRST_KEY, "v1");
        let sealed_secret = totp_keyring.seal(Uuid::now_v7(), b"twenty byte secret!!");

        assert!(matches!(
            totp_keyring.open(Uuid::now_v7(), &sealed_secret),
            Err(TotpCipherError::AuthenticationFailed)
        ));
    }

    #[test]
    fn uses_a_fresh_nonce_for_every_seal() {
        let totp_keyring = keyring(FIRST_KEY, "v1");
        let user_id = Uuid::now_v7();

        assert_ne!(
            totp_keyring.seal(user_id, b"same secret"),
            totp_keyring.seal(user_id, b"same secret")
        );
    }

    #[test]
    fn opens_a_secret_sealed_under_an_older_key_version() {
        let user_id = Uuid::now_v7();
        let sealed_with_first_key = keyring(FIRST_KEY, "v1").seal(user_id, b"old secret");
        let rotated_keyring = keyring(&format!("{FIRST_KEY},{SECOND_KEY}"), "v2");

        assert_eq!(
            rotated_keyring
                .open(user_id, &sealed_with_first_key)
                .expect("old version should still open")
                .as_slice(),
            b"old secret"
        );
        assert_eq!(rotated_keyring.seal(user_id, b"new secret")[0], 2);
    }

    #[test]
    fn refuses_a_tampered_short_or_unknown_version_secret() {
        let totp_keyring = keyring(FIRST_KEY, "v1");
        let user_id = Uuid::now_v7();
        let mut tampered_secret = totp_keyring.seal(user_id, b"secret");
        let last_byte_index = tampered_secret.len() - 1;
        tampered_secret[last_byte_index] ^= 1;
        let mut unknown_version_secret = totp_keyring.seal(user_id, b"secret");
        unknown_version_secret[0] = 9;

        assert!(matches!(
            totp_keyring.open(user_id, &tampered_secret),
            Err(TotpCipherError::AuthenticationFailed)
        ));
        assert!(matches!(
            totp_keyring.open(user_id, &[1, 2, 3]),
            Err(TotpCipherError::TooShort)
        ));
        assert!(matches!(
            totp_keyring.open(user_id, &unknown_version_secret),
            Err(TotpCipherError::UnknownKeyVersion(9))
        ));
    }

    #[test]
    fn refuses_malformed_duplicate_or_missing_current_keys() {
        assert!(TotpKeyring::parse("v1:short", "v1").is_err());
        assert!(
            TotpKeyring::parse("one:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "one").is_err()
        );
        assert!(matches!(
            TotpKeyring::parse(&format!("{FIRST_KEY},{FIRST_KEY}"), "v1"),
            Err(TotpKeyringError::DuplicateVersion(1))
        ));
        assert!(matches!(
            TotpKeyring::parse(FIRST_KEY, "v2"),
            Err(TotpKeyringError::MissingCurrentKey(_))
        ));
    }

    #[test]
    fn never_prints_key_material_in_debug_output() {
        let debug_output = format!("{:?}", keyring(FIRST_KEY, "v1"));

        assert!(!debug_output.contains("AAECAwQF"));
        assert!(debug_output.contains("current_key_version: 1"));
    }
}
