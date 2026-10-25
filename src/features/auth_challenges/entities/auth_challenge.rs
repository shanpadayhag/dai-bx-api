use uuid::Uuid;

pub const AUTH_CHALLENGE_VALID_FOR_SECONDS: i64 = 5 * 60;
pub const AUTH_CHALLENGE_COOKIE_NAME: &str = "__Host-daibx_challenge";
pub const AUTH_CHALLENGE_MAX_ATTEMPTS: i16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "auth_challenge_purpose", rename_all = "snake_case")]
pub enum AuthChallengePurpose {
    MfaVerify,
    MfaSetup,
    MfaReplace,
}

impl AuthChallengePurpose {
    pub fn needs_new_totp_secret(&self) -> bool {
        matches!(
            self,
            AuthChallengePurpose::MfaSetup | AuthChallengePurpose::MfaReplace
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AuthChallenge {
    pub id: Uuid,
    pub user_id: Uuid,
    pub purpose: AuthChallengePurpose,
    pub pending_totp_secret: Option<Vec<u8>>,
    pub attempts: i16,
}

impl AuthChallenge {
    pub fn is_out_of_attempts(&self) -> bool {
        self.attempts >= AUTH_CHALLENGE_MAX_ATTEMPTS
    }
}
