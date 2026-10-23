use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

impl HealthResponse {
    pub fn healthy() -> Self {
        Self { status: "ok" }
    }
}
