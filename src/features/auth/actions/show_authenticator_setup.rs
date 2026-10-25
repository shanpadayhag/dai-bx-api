use sqlx::PgPool;

use crate::core::security::totp::{build_otpauth_uri, encode_totp_secret};
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::core::state::AppState;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::actions::find_active_auth_challenge::FindActiveAuthChallenge;
use crate::features::users::actions::find_user_by_id::FindUserById;

const MANUAL_ENTRY_GROUP_SIZE: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub struct AuthenticatorSetup {
    pub otpauth_uri: String,
    pub manual_entry_key: String,
}

pub struct ShowAuthenticatorSetup<'a> {
    db_pool: &'a PgPool,
    totp_keyring: &'a TotpKeyring,
    find_active_auth_challenge: FindActiveAuthChallenge,
    find_user_by_id: FindUserById,
}

impl<'a> ShowAuthenticatorSetup<'a> {
    pub fn new(app_state: &'a AppState) -> Self {
        Self {
            db_pool: &app_state.db_pool,
            totp_keyring: &app_state.app_config.totp_keyring,
            find_active_auth_challenge: FindActiveAuthChallenge::new(),
            find_user_by_id: FindUserById::new(),
        }
    }

    pub async fn execute(&self, challenge_token: &str) -> Result<AuthenticatorSetup, AuthError> {
        let active_challenge = self
            .find_active_auth_challenge
            .execute(self.db_pool, challenge_token)
            .await?
            .filter(|found_challenge| found_challenge.purpose.needs_new_totp_secret())
            .ok_or(AuthError::InvalidChallenge)?;
        let sealed_totp_secret = active_challenge
            .pending_totp_secret
            .ok_or(AuthError::InvalidChallenge)?;
        let pending_totp_secret = self
            .totp_keyring
            .open(active_challenge.user_id, &sealed_totp_secret)?;
        let challenged_user = self
            .find_user_by_id
            .execute(self.db_pool, active_challenge.user_id)
            .await?
            .ok_or(AuthError::InvalidChallenge)?;
        Ok(AuthenticatorSetup {
            otpauth_uri: build_otpauth_uri(&pending_totp_secret, &challenged_user.email),
            manual_entry_key: group_for_manual_entry(&encode_totp_secret(&pending_totp_secret)),
        })
    }
}

fn group_for_manual_entry(encoded_secret: &str) -> String {
    encoded_secret
        .as_bytes()
        .chunks(MANUAL_ENTRY_GROUP_SIZE)
        .map(|secret_group| String::from_utf8_lossy(secret_group).into_owned())
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_the_key_in_fours_for_typing() {
        assert_eq!(
            group_for_manual_entry("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"),
            "ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567"
        );
    }
}
