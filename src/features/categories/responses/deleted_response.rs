use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DeletedResponse {
    pub status: &'static str,
}

impl DeletedResponse {
    pub fn deleted() -> Self {
        Self { status: "deleted" }
    }
}
