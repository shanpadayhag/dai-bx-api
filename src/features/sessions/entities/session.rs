use axum::http::HeaderValue;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::cookie::build_host_cookie;

pub const ACCESS_TOKEN_VALID_FOR_SECONDS: i64 = 15 * 60;
pub const REFRESH_TOKEN_VALID_FOR_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const SESSION_IDLE_LIMIT_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const SESSION_ABSOLUTE_LIMIT_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const REFRESH_GRACE_SECONDS: i64 = 30;
pub const ACCESS_COOKIE_NAME: &str = "__Host-daibx_access";
pub const REFRESH_COOKIE_NAME: &str = "__Host-daibx_refresh";

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "session_token_kind", rename_all = "snake_case")]
pub enum SessionTokenKind {
    Access,
    Refresh,
}

impl SessionTokenKind {
    pub fn valid_for_seconds(&self) -> i64 {
        match self {
            SessionTokenKind::Access => ACCESS_TOKEN_VALID_FOR_SECONDS,
            SessionTokenKind::Refresh => REFRESH_TOKEN_VALID_FOR_SECONDS,
        }
    }
}

pub struct IssuedSessionTokens {
    pub access_token: Zeroizing<String>,
    pub access_valid_for_seconds: i64,
    pub refresh_token: Zeroizing<String>,
    pub refresh_valid_for_seconds: i64,
}

impl IssuedSessionTokens {
    pub fn to_set_cookie_values(&self) -> [HeaderValue; 2] {
        [
            build_host_cookie(
                ACCESS_COOKIE_NAME,
                &self.access_token,
                self.access_valid_for_seconds,
            ),
            build_host_cookie(
                REFRESH_COOKIE_NAME,
                &self.refresh_token,
                self.refresh_valid_for_seconds,
            ),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct PresentedRefreshToken {
    pub token_id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub is_session_live: bool,
    pub is_replaced: bool,
    pub is_within_grace: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokedReason {
    SignedOut,
    SignedOutEverywhere,
    RefreshTokenReuse,
    PasswordReset,
    PasswordChanged,
    AuthenticatorReplaced,
}

impl RevokedReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevokedReason::SignedOut => "signed_out",
            RevokedReason::SignedOutEverywhere => "signed_out_everywhere",
            RevokedReason::RefreshTokenReuse => "refresh_token_reuse",
            RevokedReason::PasswordReset => "password_reset",
            RevokedReason::PasswordChanged => "password_changed",
            RevokedReason::AuthenticatorReplaced => "authenticator_replaced",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
}
