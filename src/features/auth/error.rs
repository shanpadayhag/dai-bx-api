use crate::core::error::AppError;
use crate::core::security::password_hash::PasswordHashError;
use crate::core::security::totp_secret_cipher::TotpCipherError;
use crate::features::auth_challenges::error::AuthChallengeError;
use crate::features::email::error::EmailError;
use crate::features::email_tokens::error::EmailTokenError;
use crate::features::invites::error::InviteError;
use crate::features::password_policy::error::PasswordPolicyError;
use crate::features::recovery_codes::error::RecoveryCodeError;
use crate::features::remembered_devices::error::RememberedDeviceError;
use crate::features::security_log::entities::security_event::ForbiddenDetail;
use crate::features::security_log::error::SecurityLogError;
use crate::features::sessions::error::SessionError;
use crate::features::throttling::error::ThrottleError;
use crate::features::users::error::UserError;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invite code is invalid or expired")]
    InvalidInvite,
    #[error("link is invalid or expired")]
    InvalidLink,
    #[error("challenge is missing, expired or used up")]
    InvalidChallenge,
    #[error("code did not match")]
    WrongCode,
    #[error("email or password is wrong")]
    InvalidCredentials,
    #[error("not signed in")]
    NotSignedIn,
    #[error(transparent)]
    PasswordPolicy(#[from] PasswordPolicyError),
    #[error("blocked for {retry_after_seconds} seconds")]
    Throttled { retry_after_seconds: u64 },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

macro_rules! auth_error_from_internal {
    ($($internal_error_type:ty),+ $(,)?) => {
        $(impl From<$internal_error_type> for AuthError {
            fn from(internal_error: $internal_error_type) -> Self {
                AuthError::Internal(internal_error.into())
            }
        })+
    };
}

auth_error_from_internal!(
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

impl From<InviteError> for AuthError {
    fn from(invite_error: InviteError) -> Self {
        match invite_error {
            InviteError::InvalidCode => AuthError::InvalidInvite,
            InviteError::Database(database_error) => AuthError::Internal(database_error.into()),
        }
    }
}

impl From<ThrottleError> for AuthError {
    fn from(throttle_error: ThrottleError) -> Self {
        match throttle_error {
            ThrottleError::Blocked {
                retry_after_seconds,
            } => AuthError::Throttled {
                retry_after_seconds,
            },
            ThrottleError::Database(database_error) => AuthError::Internal(database_error.into()),
        }
    }
}

impl From<AuthError> for AppError {
    fn from(auth_error: AuthError) -> Self {
        match auth_error {
            AuthError::InvalidInvite => {
                AppError::Validation(String::from("this invite code is invalid or has expired"))
            }
            AuthError::InvalidLink => {
                AppError::Validation(String::from("this link is invalid or has expired"))
            }
            AuthError::InvalidChallenge => AppError::Unauthorized,
            AuthError::InvalidCredentials => AppError::InvalidCredentials,
            AuthError::NotSignedIn => AppError::Unauthorized,
            AuthError::WrongCode => AppError::Validation(String::from(
                "that code didn't work, check your authenticator app",
            )),
            AuthError::PasswordPolicy(password_policy_error) => password_policy_error.into(),
            AuthError::Throttled {
                retry_after_seconds,
            } => AppError::TooManyRequests {
                retry_after_seconds,
            },
            AuthError::Internal(internal_error) => AppError::Internal(internal_error),
        }
    }
}
