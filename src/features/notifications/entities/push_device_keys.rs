use data_encoding::BASE64URL_NOPAD;
use web_push_native::p256::PublicKey;

const AUTH_SECRET_LENGTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDeviceKeys {
    pub p256dh: String,
    pub auth_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidPushDeviceKeys {
    #[error("p256dh must be a base64url P-256 public key")]
    P256dh,
    #[error("auth must be a base64url 16-byte secret")]
    AuthSecret,
}

impl PushDeviceKeys {
    pub fn parse(p256dh_text: &str, auth_secret_text: &str) -> Result<Self, InvalidPushDeviceKeys> {
        let p256dh = strip_padding(p256dh_text);
        let auth_secret = strip_padding(auth_secret_text);
        let p256dh_bytes = BASE64URL_NOPAD
            .decode(p256dh.as_bytes())
            .map_err(|_| InvalidPushDeviceKeys::P256dh)?;
        PublicKey::from_sec1_bytes(&p256dh_bytes).map_err(|_| InvalidPushDeviceKeys::P256dh)?;
        if p256dh_bytes.first() != Some(&0x04) {
            return Err(InvalidPushDeviceKeys::P256dh);
        }
        let auth_secret_bytes = BASE64URL_NOPAD
            .decode(auth_secret.as_bytes())
            .map_err(|_| InvalidPushDeviceKeys::AuthSecret)?;
        if auth_secret_bytes.len() != AUTH_SECRET_LENGTH {
            return Err(InvalidPushDeviceKeys::AuthSecret);
        }
        Ok(Self {
            p256dh: p256dh.to_owned(),
            auth_secret: auth_secret.to_owned(),
        })
    }
}

fn strip_padding(key_text: &str) -> &str {
    key_text.trim().trim_end_matches('=')
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_P256DH: &str =
        "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
    const SAMPLE_AUTH_SECRET: &str = "tBHItJI5svbpez7KI4CCXg";

    #[test]
    fn accepts_browser_keys_with_or_without_padding() {
        let padded_keys = PushDeviceKeys::parse(
            &format!("{SAMPLE_P256DH}="),
            &format!("{SAMPLE_AUTH_SECRET}=="),
        )
        .expect("padded keys should parse");

        assert_eq!(padded_keys.p256dh, SAMPLE_P256DH);
        assert_eq!(padded_keys.auth_secret, SAMPLE_AUTH_SECRET);
    }

    #[test]
    fn rejects_a_key_that_is_not_a_point_on_the_curve() {
        let off_curve_key = format!("BA{}", &SAMPLE_P256DH[2..]);

        assert_eq!(
            PushDeviceKeys::parse(&off_curve_key, SAMPLE_AUTH_SECRET),
            Err(InvalidPushDeviceKeys::P256dh)
        );
        assert_eq!(
            PushDeviceKeys::parse("not+base64url", SAMPLE_AUTH_SECRET),
            Err(InvalidPushDeviceKeys::P256dh)
        );
    }

    #[test]
    fn rejects_an_auth_secret_of_the_wrong_length() {
        assert_eq!(
            PushDeviceKeys::parse(SAMPLE_P256DH, "c2hvcnQ"),
            Err(InvalidPushDeviceKeys::AuthSecret)
        );
    }
}
