use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use zeroize::Zeroizing;

use crate::core::security::constant_time::bytes_match;
use crate::core::security::random_bytes::generate_random_bytes;

pub const TOTP_STEP_SECONDS: u64 = 30;
pub const TOTP_DIGIT_COUNT: usize = 6;
pub const TOTP_SECRET_BYTE_COUNT: usize = 20;
pub const TOTP_ISSUER: &str = "DaiBX";
const TOTP_MODULUS: u32 = 1_000_000;
const ACCEPTED_STEP_DRIFT: u64 = 1;

pub fn generate_totp_secret() -> Zeroizing<Vec<u8>> {
    Zeroizing::new(generate_random_bytes::<TOTP_SECRET_BYTE_COUNT>().to_vec())
}

pub fn totp_step_at(unix_seconds: u64) -> u64 {
    unix_seconds / TOTP_STEP_SECONDS
}

pub fn totp_code_at_step(totp_secret: &[u8], totp_step: u64) -> String {
    let mut step_mac =
        Hmac::<Sha1>::new_from_slice(totp_secret).expect("HMAC accepts keys of any length");
    step_mac.update(&totp_step.to_be_bytes());
    let step_digest = step_mac.finalize().into_bytes();
    let truncation_offset = usize::from(step_digest[step_digest.len() - 1] & 0x0f);
    let truncated_value = u32::from_be_bytes([
        step_digest[truncation_offset] & 0x7f,
        step_digest[truncation_offset + 1],
        step_digest[truncation_offset + 2],
        step_digest[truncation_offset + 3],
    ]);
    format!(
        "{:0width$}",
        truncated_value % TOTP_MODULUS,
        width = TOTP_DIGIT_COUNT
    )
}

pub fn find_accepted_totp_step(
    totp_secret: &[u8],
    entered_code: &str,
    unix_seconds: u64,
    last_used_step: Option<u64>,
) -> Option<u64> {
    let is_well_formed_code = entered_code.len() == TOTP_DIGIT_COUNT
        && entered_code
            .bytes()
            .all(|code_byte| code_byte.is_ascii_digit());
    if !is_well_formed_code {
        return None;
    }
    let current_step = totp_step_at(unix_seconds);
    (current_step.saturating_sub(ACCEPTED_STEP_DRIFT)..=current_step + ACCEPTED_STEP_DRIFT)
        .filter(|candidate_step| last_used_step.is_none_or(|used_step| *candidate_step > used_step))
        .find(|candidate_step| {
            bytes_match(
                totp_code_at_step(totp_secret, *candidate_step).as_bytes(),
                entered_code.as_bytes(),
            )
        })
}

pub fn encode_totp_secret(totp_secret: &[u8]) -> String {
    BASE32_NOPAD.encode(totp_secret)
}

pub fn build_otpauth_uri(totp_secret: &[u8], account_email: &str) -> String {
    format!(
        "otpauth://totp/{TOTP_ISSUER}:{}?secret={}&issuer={TOTP_ISSUER}&algorithm=SHA1&digits={TOTP_DIGIT_COUNT}&period={TOTP_STEP_SECONDS}",
        percent_encode(account_email),
        encode_totp_secret(totp_secret),
    )
}

fn percent_encode(uri_component: &str) -> String {
    uri_component
        .bytes()
        .map(|component_byte| match component_byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(component_byte).to_string()
            }
            _ => format!("%{component_byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_6238_SHA1_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn matches_the_rfc_6238_sha1_test_vectors() {
        for (unix_seconds, expected_code) in [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_111_111_111, "050471"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
            (20_000_000_000, "353130"),
        ] {
            assert_eq!(
                totp_code_at_step(RFC_6238_SHA1_SECRET, totp_step_at(unix_seconds)),
                expected_code,
                "code at {unix_seconds}"
            );
        }
    }

    #[test]
    fn generates_a_20_byte_secret_that_differs_each_time() {
        let first_secret = generate_totp_secret();

        assert_eq!(first_secret.len(), TOTP_SECRET_BYTE_COUNT);
        assert_ne!(*first_secret, *generate_totp_secret());
    }

    #[test]
    fn accepts_the_current_code_and_one_step_either_side() {
        let now_unix_seconds = 1_234_567_890;
        let current_step = totp_step_at(now_unix_seconds);

        for accepted_step in [current_step - 1, current_step, current_step + 1] {
            let entered_code = totp_code_at_step(RFC_6238_SHA1_SECRET, accepted_step);
            assert_eq!(
                find_accepted_totp_step(
                    RFC_6238_SHA1_SECRET,
                    &entered_code,
                    now_unix_seconds,
                    None
                ),
                Some(accepted_step)
            );
        }
    }

    #[test]
    fn refuses_a_code_two_steps_away_or_wrong() {
        let now_unix_seconds = 1_234_567_890;
        let stale_code =
            totp_code_at_step(RFC_6238_SHA1_SECRET, totp_step_at(now_unix_seconds) - 2);

        assert_eq!(
            find_accepted_totp_step(RFC_6238_SHA1_SECRET, &stale_code, now_unix_seconds, None),
            None
        );
        assert_eq!(
            find_accepted_totp_step(RFC_6238_SHA1_SECRET, "000000", now_unix_seconds, None),
            None
        );
        assert_eq!(
            find_accepted_totp_step(RFC_6238_SHA1_SECRET, "12345", now_unix_seconds, None),
            None
        );
    }

    #[test]
    fn refuses_a_code_at_or_before_the_last_used_step() {
        let now_unix_seconds = 1_234_567_890;
        let current_step = totp_step_at(now_unix_seconds);
        let current_code = totp_code_at_step(RFC_6238_SHA1_SECRET, current_step);
        let next_code = totp_code_at_step(RFC_6238_SHA1_SECRET, current_step + 1);

        assert_eq!(
            find_accepted_totp_step(
                RFC_6238_SHA1_SECRET,
                &current_code,
                now_unix_seconds,
                Some(current_step)
            ),
            None
        );
        assert_eq!(
            find_accepted_totp_step(
                RFC_6238_SHA1_SECRET,
                &next_code,
                now_unix_seconds,
                Some(current_step)
            ),
            Some(current_step + 1)
        );
    }

    #[test]
    fn encodes_the_secret_as_unpadded_base32() {
        assert_eq!(
            encode_totp_secret(RFC_6238_SHA1_SECRET),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );
    }

    #[test]
    fn builds_an_otpauth_uri_with_the_issuer_and_escaped_email() {
        assert_eq!(
            build_otpauth_uri(RFC_6238_SHA1_SECRET, "you+tag@example.com"),
            "otpauth://totp/DaiBX:you%2Btag%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=DaiBX&algorithm=SHA1&digits=6&period=30"
        );
    }
}
