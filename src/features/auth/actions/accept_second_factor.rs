use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::security::totp::TOTP_DIGIT_COUNT;
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::features::auth::error::AuthError;
use crate::features::recovery_codes::actions::use_recovery_code::UseRecoveryCode;
use crate::features::users::actions::accept_totp_code::AcceptTotpCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptedSecondFactor {
    Authenticator,
    RecoveryCode { remaining_count: i64 },
}

impl AcceptedSecondFactor {
    pub fn method_name(&self) -> &'static str {
        match self {
            AcceptedSecondFactor::Authenticator => "authenticator",
            AcceptedSecondFactor::RecoveryCode { .. } => "recovery_code",
        }
    }

    pub fn remaining_recovery_codes(&self) -> Option<i64> {
        match self {
            AcceptedSecondFactor::Authenticator => None,
            AcceptedSecondFactor::RecoveryCode { remaining_count } => Some(*remaining_count),
        }
    }
}

pub struct AcceptSecondFactor<'a> {
    accept_totp_code: AcceptTotpCode<'a>,
    use_recovery_code: UseRecoveryCode,
}

impl<'a> AcceptSecondFactor<'a> {
    pub fn new(totp_keyring: &'a TotpKeyring) -> Self {
        Self {
            accept_totp_code: AcceptTotpCode::new(totp_keyring),
            use_recovery_code: UseRecoveryCode::new(),
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        entered_code: &str,
    ) -> Result<Option<AcceptedSecondFactor>, AuthError> {
        let is_authenticator_code = entered_code.len() == TOTP_DIGIT_COUNT
            && entered_code
                .bytes()
                .all(|code_byte| code_byte.is_ascii_digit());
        if is_authenticator_code {
            let is_accepted = self
                .accept_totp_code
                .execute(&mut *connection, user_id, entered_code)
                .await?;
            return Ok(is_accepted.then_some(AcceptedSecondFactor::Authenticator));
        }
        Ok(self
            .use_recovery_code
            .execute(&mut *connection, user_id, entered_code)
            .await?
            .map(|remaining_count| AcceptedSecondFactor::RecoveryCode { remaining_count }))
    }
}
