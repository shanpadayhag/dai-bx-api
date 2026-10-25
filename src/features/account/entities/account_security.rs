#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct AccountSecurity {
    pub recovery_codes_remaining: i64,
    pub active_session_count: i64,
}
