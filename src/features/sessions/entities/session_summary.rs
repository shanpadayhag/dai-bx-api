use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SessionSummary {
    pub id: Uuid,
    pub created_at: String,
    pub last_seen_at: String,
    pub user_agent: Option<String>,
}
