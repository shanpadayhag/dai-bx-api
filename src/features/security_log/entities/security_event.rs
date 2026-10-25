use std::net::IpAddr;

use serde_json::{Map, Value};
use uuid::Uuid;

pub const MAX_USER_AGENT_CHARACTERS: usize = 512;
const FORBIDDEN_DETAIL_FRAGMENTS: [&str; 10] = [
    "password", "token", "code", "secret", "totp", "otp", "email", "cookie", "hash", "salt",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "security_event_type", rename_all = "snake_case")]
pub enum SecurityEventType {
    SignUp,
    EmailVerified,
    SignInSucceeded,
    SignInFailed,
    MfaSetUp,
    RecoveryCodeUsed,
    RecoveryCodesRegenerated,
    PasswordChanged,
    PasswordResetRequested,
    PasswordResetCompleted,
    EmailChanged,
    SessionRevoked,
    SignedOutEverywhere,
    RefreshTokenReuseDetected,
    InviteUsed,
    AccountDeleted,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("security event detail {0:?} could hold a secret or personal data")]
pub struct ForbiddenDetail(pub String);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SecurityEventDetails {
    detail_values: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct NewSecurityEvent {
    pub event_type: SecurityEventType,
    pub subject_ref: Option<Uuid>,
    pub client_ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub details: SecurityEventDetails,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SecurityEvent {
    pub id: Uuid,
    pub event_type: SecurityEventType,
    pub subject_ref: Option<Uuid>,
    pub client_ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub details: Value,
}

impl SecurityEventDetails {
    pub fn with_text(self, detail_name: &str, detail_text: &str) -> Result<Self, ForbiddenDetail> {
        if detail_text.contains('@') {
            return Err(ForbiddenDetail(detail_name.to_owned()));
        }
        self.with_value(detail_name, Value::from(detail_text))
    }

    pub fn with_number(
        self,
        detail_name: &str,
        detail_number: i64,
    ) -> Result<Self, ForbiddenDetail> {
        self.with_value(detail_name, Value::from(detail_number))
    }

    pub fn with_flag(self, detail_name: &str, detail_flag: bool) -> Result<Self, ForbiddenDetail> {
        self.with_value(detail_name, Value::from(detail_flag))
    }

    fn with_value(
        mut self,
        detail_name: &str,
        detail_value: Value,
    ) -> Result<Self, ForbiddenDetail> {
        let lowercase_name = detail_name.to_ascii_lowercase();
        if FORBIDDEN_DETAIL_FRAGMENTS
            .iter()
            .any(|forbidden_fragment| lowercase_name.contains(forbidden_fragment))
        {
            return Err(ForbiddenDetail(detail_name.to_owned()));
        }
        self.detail_values
            .insert(detail_name.to_owned(), detail_value);
        Ok(self)
    }

    pub fn to_json(&self) -> Value {
        Value::Object(self.detail_values.clone())
    }
}

impl NewSecurityEvent {
    pub fn trimmed_user_agent(&self) -> Option<String> {
        self.user_agent
            .as_ref()
            .map(|user_agent| user_agent.chars().take(MAX_USER_AGENT_CHARACTERS).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_harmless_details_as_json() {
        let security_event_details = SecurityEventDetails::default()
            .with_text("revoked_reason", "signed_out")
            .and_then(|details| details.with_number("remaining_count", 7))
            .and_then(|details| details.with_flag("remembered_device", true))
            .expect("harmless details should be accepted");

        assert_eq!(
            security_event_details.to_json(),
            serde_json::json!({
                "revoked_reason": "signed_out",
                "remaining_count": 7,
                "remembered_device": true,
            })
        );
    }

    #[test]
    fn refuses_detail_names_that_could_hold_a_secret() {
        for forbidden_name in [
            "password",
            "new_password_hash",
            "refresh_token",
            "recovery_code",
            "totp_secret",
            "one_time_otp",
            "email",
            "new_email",
            "session_cookie",
            "salt",
        ] {
            assert_eq!(
                SecurityEventDetails::default().with_flag(forbidden_name, true),
                Err(ForbiddenDetail(forbidden_name.to_owned())),
                "{forbidden_name} should be refused"
            );
        }
    }

    #[test]
    fn refuses_text_that_looks_like_an_email_address() {
        assert!(
            SecurityEventDetails::default()
                .with_text("revoked_reason", "you@example.com")
                .is_err()
        );
    }

    #[test]
    fn caps_the_user_agent_at_512_characters() {
        let new_security_event = NewSecurityEvent {
            event_type: SecurityEventType::SignInFailed,
            subject_ref: None,
            client_ip: None,
            user_agent: Some("é".repeat(600)),
            details: SecurityEventDetails::default(),
        };

        assert_eq!(
            new_security_event
                .trimmed_user_agent()
                .expect("user agent should be kept")
                .chars()
                .count(),
            MAX_USER_AGENT_CHARACTERS
        );
    }
}
