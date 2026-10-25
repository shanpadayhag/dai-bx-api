use crate::core::error::AppError;
use crate::features::password_policy::entities::new_password::{
    MAX_PASSWORD_CHARACTERS, MIN_PASSWORD_CHARACTERS, PasswordRejection,
};

#[derive(Debug, thiserror::Error)]
pub enum PasswordPolicyError {
    #[error("password rejected: {0:?}")]
    Rejected(PasswordRejection),
}

impl From<PasswordPolicyError> for AppError {
    fn from(password_policy_error: PasswordPolicyError) -> Self {
        match password_policy_error {
            PasswordPolicyError::Rejected(password_rejection) => {
                AppError::Validation(rejection_message(password_rejection))
            }
        }
    }
}

fn rejection_message(password_rejection: PasswordRejection) -> String {
    match password_rejection {
        PasswordRejection::TooShort => {
            format!("password must be at least {MIN_PASSWORD_CHARACTERS} characters")
        }
        PasswordRejection::TooLong => {
            format!("password must be at most {MAX_PASSWORD_CHARACTERS} characters")
        }
        PasswordRejection::Common => String::from("this password is too common"),
        PasswordRejection::ContainsPersonalWord => {
            String::from("password can't contain DaiBX, your name or your email")
        }
        PasswordRejection::Breached => {
            String::from("this password has appeared in a data breach, so choose another")
        }
    }
}
