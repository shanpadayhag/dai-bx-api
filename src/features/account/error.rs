use crate::core::error::AppError;
use crate::core::security::password_hash::PasswordHashError;
use crate::core::security::totp_secret_cipher::TotpCipherError;
use crate::features::account::entities::account_profile::ACCOUNT_DELETION_CONFIRMATION;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::error::AuthChallengeError;
use crate::features::email::error::EmailError;
use crate::features::email_tokens::error::EmailTokenError;
use crate::features::password_policy::error::PasswordPolicyError;
use crate::features::recovery_codes::error::RecoveryCodeError;
use crate::features::remembered_devices::error::RememberedDeviceError;
use crate::features::security_log::entities::security_event::ForbiddenDetail;
use crate::features::security_log::error::SecurityLogError;
use crate::features::sessions::error::SessionError;
use crate::features::throttling::error::ThrottleError;
use crate::features::users::error::UserError;

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("{0}")]
    Invalid(String),
    #[error("current password is wrong")]
    WrongCurrentPassword,
    #[error("password is wrong")]
    WrongPassword,
    #[error("code did not match")]
    WrongCode,
    #[error("new email is the current email")]
    SameEmail,
    #[error("email is taken")]
    EmailTaken,
    #[error("link is invalid or expired")]
    InvalidLink,
    #[error("replacement step is missing, expired or used up")]
    StepExpired,
    #[error("confirmation text does not match")]
    WrongConfirmation,
    #[error("account no longer exists")]
    NotSignedIn,
    #[error(transparent)]
    PasswordPolicy(#[from] PasswordPolicyError),
    #[error("blocked for {retry_after_seconds} seconds")]
    Throttled { retry_after_seconds: u64 },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

macro_rules! account_error_from_internal {
    ($($internal_error_type:ty),+ $(,)?) => {
        $(impl From<$internal_error_type> for AccountError {
            fn from(internal_error: $internal_error_type) -> Self {
                AccountError::Internal(internal_error.into())
            }
        })+
    };
}

account_error_from_internal!(
    AuthChallengeError,
    EmailError,
    EmailTokenError,
    ForbiddenDetail,
    PasswordHashError,
    RecoveryCodeError,
    RememberedDeviceError,
    SecurityLogError,
    SessionError,
    TotpCipherError,
    UserError,
    sqlx::Error,
);

impl From<ThrottleError> for AccountError {
    fn from(throttle_error: ThrottleError) -> Self {
        match throttle_error {
            ThrottleError::Blocked {
                retry_after_seconds,
            } => AccountError::Throttled {
                retry_after_seconds,
            },
            ThrottleError::Database(database_error) => {
                AccountError::Internal(database_error.into())
            }
        }
    }
}

impl From<AuthError> for AccountError {
    fn from(auth_error: AuthError) -> Self {
        match auth_error {
            AuthError::Throttled {
                retry_after_seconds,
            } => AccountError::Throttled {
                retry_after_seconds,
            },
            AuthError::PasswordPolicy(password_policy_error) => {
                AccountError::PasswordPolicy(password_policy_error)
            }
            other_auth_error => AccountError::Internal(other_auth_error.into()),
        }
    }
}

impl From<AccountError> for AppError {
    fn from(account_error: AccountError) -> Self {
        match account_error {
            AccountError::Invalid(invalid_message) => AppError::Validation(invalid_message),
            AccountError::WrongCurrentPassword => {
                AppError::Validation(String::from("your current password is incorrect"))
            }
            AccountError::WrongPassword => {
                AppError::Validation(String::from("your password is incorrect"))
            }
            AccountError::WrongCode => AppError::Validation(String::from(
                "that code didn't work, check your authenticator app",
            )),
            AccountError::SameEmail => {
                AppError::Validation(String::from("that's already your email address"))
            }
            AccountError::EmailTaken => {
                AppError::Conflict(String::from("that email address is already in use"))
            }
            AccountError::InvalidLink => {
                AppError::Validation(String::from("this link is invalid or has expired"))
            }
            AccountError::StepExpired => {
                AppError::Validation(String::from("this step has expired, start again"))
            }
            AccountError::WrongConfirmation => AppError::Validation(format!(
                "type \"{ACCOUNT_DELETION_CONFIRMATION}\" to confirm"
            )),
            AccountError::NotSignedIn => AppError::Unauthorized,
            AccountError::PasswordPolicy(password_policy_error) => password_policy_error.into(),
            AccountError::Throttled {
                retry_after_seconds,
            } => AppError::TooManyRequests {
                retry_after_seconds,
            },
            AccountError::Internal(internal_error) => AppError::Internal(internal_error),
        }
    }
}
