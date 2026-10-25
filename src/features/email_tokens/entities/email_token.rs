use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "email_token_purpose", rename_all = "snake_case")]
pub enum EmailTokenPurpose {
    VerifyEmail,
    PasswordReset,
    ChangeEmail,
}

impl EmailTokenPurpose {
    pub fn valid_for_seconds(&self) -> i64 {
        match self {
            EmailTokenPurpose::VerifyEmail | EmailTokenPurpose::ChangeEmail => 24 * 60 * 60,
            EmailTokenPurpose::PasswordReset => 30 * 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedEmailToken {
    pub user_id: Uuid,
    pub new_email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_verify_and_change_links_a_day_and_reset_links_half_an_hour() {
        assert_eq!(EmailTokenPurpose::VerifyEmail.valid_for_seconds(), 86_400);
        assert_eq!(EmailTokenPurpose::ChangeEmail.valid_for_seconds(), 86_400);
        assert_eq!(EmailTokenPurpose::PasswordReset.valid_for_seconds(), 1_800);
    }
}
