use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub security_ref: Uuid,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub is_email_verified: bool,
    pub is_mfa_enabled: bool,
    pub terms_version: String,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub terms_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TotpState {
    pub sealed_totp_secret: Option<Vec<u8>>,
    pub last_used_step: Option<i64>,
}
