use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SentResponse {
    pub status: &'static str,
}

impl SentResponse {
    pub fn sent() -> Self {
        Self { status: "sent" }
    }
}
