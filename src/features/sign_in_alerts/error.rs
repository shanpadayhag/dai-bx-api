use crate::core::error::AppError;
use crate::features::security_log::error::SecurityLogError;
use crate::features::throttling::error::ThrottleError;

#[derive(Debug, thiserror::Error)]
pub enum SignInAlertError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    SecurityLog(#[from] SecurityLogError),
    #[error(transparent)]
    Throttle(#[from] ThrottleError),
}

impl From<SignInAlertError> for AppError {
    fn from(sign_in_alert_error: SignInAlertError) -> Self {
        AppError::Internal(sign_in_alert_error.into())
    }
}
