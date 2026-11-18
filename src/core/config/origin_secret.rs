use std::fmt;

use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const MIN_ORIGIN_SECRET_LENGTH: usize = 32;

#[derive(Clone)]
pub struct OriginSecret(Zeroizing<String>);

impl OriginSecret {
    pub fn new(origin_secret_text: String) -> Option<Self> {
        (origin_secret_text.len() >= MIN_ORIGIN_SECRET_LENGTH)
            .then(|| Self(Zeroizing::new(origin_secret_text)))
    }

    pub fn is_matched_by(&self, presented_secret: &[u8]) -> bool {
        self.0.as_bytes().ct_eq(presented_secret).into()
    }
}

impl fmt::Debug for OriginSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OriginSecret([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn origin_secret() -> OriginSecret {
        OriginSecret::new(EXPECTED_SECRET.to_owned()).expect("32 characters is long enough")
    }

    #[test]
    fn matches_only_the_exact_secret() {
        assert!(origin_secret().is_matched_by(EXPECTED_SECRET.as_bytes()));
        assert!(!origin_secret().is_matched_by(b"0123456789abcdef0123456789abcdeF"));
        assert!(!origin_secret().is_matched_by(b"0123456789abcdef0123456789abcde"));
        assert!(!origin_secret().is_matched_by(b""));
    }

    #[test]
    fn refuses_a_secret_shorter_than_32_characters() {
        assert!(OriginSecret::new(EXPECTED_SECRET[1..].to_owned()).is_none());
    }

    #[test]
    fn never_prints_the_secret() {
        assert!(!format!("{:?}", origin_secret()).contains(EXPECTED_SECRET));
    }
}
