use sqlx::PgExecutor;

use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::features::auth::actions::sign_in::SignInOutcome;
use crate::features::auth::error::AuthError;
use crate::features::auth_challenges::actions::create_auth_challenge::CreateAuthChallenge;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallengePurpose;
use crate::features::users::entities::user::User;

pub struct ChallengeAfterPassword<'a> {
    create_auth_challenge: CreateAuthChallenge<'a>,
}

impl<'a> ChallengeAfterPassword<'a> {
    pub fn new(totp_keyring: &'a TotpKeyring) -> Self {
        Self {
            create_auth_challenge: CreateAuthChallenge::new(totp_keyring),
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        password_holder: &User,
    ) -> Result<SignInOutcome, AuthError> {
        if !password_holder.is_email_verified {
            return Ok(SignInOutcome::VerifyEmail);
        }
        if !password_holder.is_mfa_enabled {
            let challenge_token = self
                .create_auth_challenge
                .execute(executor, password_holder.id, AuthChallengePurpose::MfaSetup)
                .await?;
            return Ok(SignInOutcome::SetUpAuthenticator { challenge_token });
        }
        let challenge_token = self
            .create_auth_challenge
            .execute(
                executor,
                password_holder.id,
                AuthChallengePurpose::MfaVerify,
            )
            .await?;
        Ok(SignInOutcome::EnterCode { challenge_token })
    }
}
