use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredRecoveryCode {
    pub id: Uuid,
    pub salt: Vec<u8>,
    pub code_hash: Vec<u8>,
}
