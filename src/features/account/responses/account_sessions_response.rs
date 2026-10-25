use serde::Serialize;
use uuid::Uuid;

use crate::features::sessions::entities::session_summary::SessionSummary;

#[derive(Debug, Serialize)]
pub struct AccountSessionsResponse {
    pub sessions: Vec<AccountSessionResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSessionResponse {
    pub id: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub user_agent: Option<String>,
    pub is_current: bool,
}

impl AccountSessionsResponse {
    pub fn new(session_summaries: Vec<SessionSummary>, current_session_id: Uuid) -> Self {
        Self {
            sessions: session_summaries
                .into_iter()
                .map(|session_summary| AccountSessionResponse {
                    is_current: session_summary.id == current_session_id,
                    id: session_summary.id.to_string(),
                    created_at: session_summary.created_at,
                    last_seen_at: session_summary.last_seen_at,
                    user_agent: session_summary.user_agent,
                })
                .collect(),
        }
    }
}
