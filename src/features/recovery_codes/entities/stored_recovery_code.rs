use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredRecoveryCode {
    pub id: Uuid,
    pub code_phc: Option<String>,
    pub legacy_salt: Option<Vec<u8>>,
    pub legacy_code_hash: Option<Vec<u8>>,
}
