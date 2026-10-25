use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedInResponse {
    pub next: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_recovery_codes: Option<i64>,
}

impl SignedInResponse {
    pub fn new(remaining_recovery_codes: Option<i64>) -> Self {
        Self {
            next: "signed_in",
            remaining_recovery_codes,
        }
    }
}
