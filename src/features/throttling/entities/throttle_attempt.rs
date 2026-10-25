use std::net::IpAddr;

use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};

pub const FREE_ACCOUNT_IP_FAILURES: i32 = 5;
pub const FIRST_BLOCK_SECONDS: i64 = 60;
pub const MAX_BLOCK_SECONDS: i64 = 15 * 60;
pub const LOOSE_FAILURE_LIMIT: i32 = 100;
pub const LOOSE_WINDOW_SECONDS: i64 = 60 * 60;
pub const EMAIL_SEND_LIMIT: i32 = 3;
pub const EMAIL_SEND_WINDOW_SECONDS: i64 = 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleScope {
    SignIn,
    CodeEntry,
    PasswordReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSendPurpose {
    VerifyEmail,
    PasswordReset,
    ChangeEmail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrottleAttempt {
    pub scope: ThrottleScope,
    pub account_key: String,
    pub client_ip: Option<IpAddr>,
    pub has_valid_remembered_device: bool,
}

pub fn email_account_key(email: &str) -> String {
    HEXLOWER.encode(&Sha256::digest(email.trim().to_lowercase().as_bytes()))
}

pub fn email_send_key(purpose: EmailSendPurpose, email: &str) -> String {
    format!(
        "email_send:{}:{}",
        purpose.key_prefix(),
        email_account_key(email)
    )
}

pub fn account_ip_block_seconds(failure_count: i32) -> Option<i64> {
    let blocked_failure_count = failure_count - FREE_ACCOUNT_IP_FAILURES;
    if blocked_failure_count < 1 {
        return None;
    }
    let doubling_count = u32::try_from(blocked_failure_count - 1).unwrap_or(u32::MAX);
    Some(
        FIRST_BLOCK_SECONDS
            .checked_mul(2_i64.checked_pow(doubling_count).unwrap_or(i64::MAX))
            .unwrap_or(i64::MAX)
            .min(MAX_BLOCK_SECONDS),
    )
}

impl ThrottleScope {
    fn key_prefix(&self) -> &'static str {
        match self {
            ThrottleScope::SignIn => "signin",
            ThrottleScope::CodeEntry => "mfa",
            ThrottleScope::PasswordReset => "reset",
        }
    }
}

impl EmailSendPurpose {
    fn key_prefix(&self) -> &'static str {
        match self {
            EmailSendPurpose::VerifyEmail => "verify_email",
            EmailSendPurpose::PasswordReset => "password_reset",
            EmailSendPurpose::ChangeEmail => "change_email",
        }
    }
}

impl ThrottleAttempt {
    pub fn account_ip_key(&self) -> String {
        let client_ip_text = self.client_ip.map_or_else(
            || String::from("unknown"),
            |client_ip| client_ip.to_string(),
        );
        format!(
            "{}:acct_ip:{}:{client_ip_text}",
            self.scope.key_prefix(),
            self.account_key
        )
    }

    pub fn account_key_name(&self) -> String {
        format!("{}:acct:{}", self.scope.key_prefix(), self.account_key)
    }

    pub fn ip_key(&self) -> Option<String> {
        self.client_ip
            .map(|client_ip| format!("{}:ip:{client_ip}", self.scope.key_prefix()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt_from(client_ip: Option<&str>) -> ThrottleAttempt {
        ThrottleAttempt {
            scope: ThrottleScope::SignIn,
            account_key: email_account_key("Person@Example.com"),
            client_ip: client_ip.map(|address| address.parse().expect("valid address")),
            has_valid_remembered_device: false,
        }
    }

    #[test]
    fn keys_an_account_by_the_sha256_of_its_trimmed_lowercased_email() {
        assert_eq!(
            email_account_key(" Person@Example.com "),
            email_account_key("person@example.com")
        );
        assert_eq!(email_account_key("person@example.com").len(), 64);
        assert_ne!(
            email_account_key("person@example.com"),
            email_account_key("other@example.com")
        );
    }

    #[test]
    fn names_keys_by_scope_account_and_ip() {
        let throttle_attempt = attempt_from(Some("180.191.77.129"));
        let account_key = email_account_key("person@example.com");

        assert_eq!(
            throttle_attempt.account_ip_key(),
            format!("signin:acct_ip:{account_key}:180.191.77.129")
        );
        assert_eq!(
            throttle_attempt.account_key_name(),
            format!("signin:acct:{account_key}")
        );
        assert_eq!(
            throttle_attempt.ip_key().as_deref(),
            Some("signin:ip:180.191.77.129")
        );
    }

    #[test]
    fn uses_an_unknown_ip_marker_and_no_ip_key_without_an_address() {
        let throttle_attempt = attempt_from(None);

        assert!(throttle_attempt.account_ip_key().ends_with(":unknown"));
        assert_eq!(throttle_attempt.ip_key(), None);
    }

    #[test]
    fn keeps_scopes_and_email_purposes_apart() {
        let code_entry_attempt = ThrottleAttempt {
            scope: ThrottleScope::CodeEntry,
            ..attempt_from(None)
        };

        assert!(
            code_entry_attempt
                .account_key_name()
                .starts_with("mfa:acct:")
        );
        assert_ne!(
            email_send_key(EmailSendPurpose::VerifyEmail, "a@b.co"),
            email_send_key(EmailSendPurpose::PasswordReset, "a@b.co")
        );
        assert!(
            email_send_key(EmailSendPurpose::ChangeEmail, "a@b.co")
                .starts_with("email_send:change_email:")
        );
    }

    #[test]
    fn blocks_from_the_sixth_failure_doubling_from_a_minute_up_to_fifteen() {
        assert_eq!(account_ip_block_seconds(5), None);
        assert_eq!(account_ip_block_seconds(6), Some(60));
        assert_eq!(account_ip_block_seconds(7), Some(120));
        assert_eq!(account_ip_block_seconds(8), Some(240));
        assert_eq!(account_ip_block_seconds(9), Some(480));
        assert_eq!(account_ip_block_seconds(10), Some(900));
        assert_eq!(account_ip_block_seconds(64), Some(900));
    }
}
