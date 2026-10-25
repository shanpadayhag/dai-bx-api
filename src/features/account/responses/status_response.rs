use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
}

impl StatusResponse {
    pub fn password_changed() -> Self {
        Self {
            status: "password_changed",
        }
    }

    pub fn confirmation_email_sent() -> Self {
        Self {
            status: "confirmation_email_sent",
        }
    }

    pub fn email_changed() -> Self {
        Self {
            status: "email_changed",
        }
    }

    pub fn authenticator_replaced() -> Self {
        Self {
            status: "authenticator_replaced",
        }
    }

    pub fn account_deleted() -> Self {
        Self {
            status: "account_deleted",
        }
    }
}
