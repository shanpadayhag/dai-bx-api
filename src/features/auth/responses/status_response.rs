use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
}

impl StatusResponse {
    pub fn verification_email_sent() -> Self {
        Self {
            status: "verification_email_sent",
        }
    }

    pub fn reset_email_sent() -> Self {
        Self {
            status: "reset_email_sent",
        }
    }

    pub fn refreshed() -> Self {
        Self {
            status: "refreshed",
        }
    }

    pub fn signed_out() -> Self {
        Self {
            status: "signed_out",
        }
    }
}
