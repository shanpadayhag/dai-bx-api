use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NextStepResponse {
    pub next: &'static str,
}

impl NextStepResponse {
    pub fn set_up_authenticator() -> Self {
        Self {
            next: "set_up_authenticator",
        }
    }

    pub fn verify_email() -> Self {
        Self {
            next: "verify_email",
        }
    }

    pub fn enter_code() -> Self {
        Self { next: "enter_code" }
    }

    pub fn signed_in() -> Self {
        Self { next: "signed_in" }
    }
}
