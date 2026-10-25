use sqlx::PgExecutor;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::core::security::opaque_token::{generate_opaque_token, hash_opaque_token};
use crate::core::security::totp::generate_totp_secret;
use crate::core::security::totp_secret_cipher::TotpKeyring;
use crate::features::auth_challenges::data_sources::postgres::auth_challenge_source::PostgresAuthChallengeSource;
use crate::features::auth_challenges::entities::auth_challenge::{
    AUTH_CHALLENGE_VALID_FOR_SECONDS, AuthChallengePurpose,
};
use crate::features::auth_challenges::error::AuthChallengeError;

pub struct CreateAuthChallenge<'a> {
    totp_keyring: &'a TotpKeyring,
    postgres_auth_challenge_source: PostgresAuthChallengeSource,
}

impl<'a> CreateAuthChallenge<'a> {
    pub fn new(totp_keyring: &'a TotpKeyring) -> Self {
        Self {
            totp_keyring,
            postgres_auth_challenge_source: PostgresAuthChallengeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        purpose: AuthChallengePurpose,
    ) -> Result<Zeroizing<String>, AuthChallengeError> {
        let sealed_totp_secret = purpose
            .needs_new_totp_secret()
            .then(|| self.totp_keyring.seal(user_id, &generate_totp_secret()));
        let challenge_token = generate_opaque_token();
        self.postgres_auth_challenge_source
            .insert(
                executor,
                user_id,
                purpose,
                &hash_opaque_token(&challenge_token),
                sealed_totp_secret.as_deref(),
                AUTH_CHALLENGE_VALID_FOR_SECONDS,
            )
            .await?;
        Ok(challenge_token)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::security::totp::TOTP_SECRET_BYTE_COUNT;
    use crate::features::auth_challenges::actions::find_active_auth_challenge::FindActiveAuthChallenge;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    fn test_keyring() -> TotpKeyring {
        TotpKeyring::parse("v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "v1")
            .expect("test keyring should parse")
    }

    async fn seed_user_id(db_pool: &PgPool) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: format!("{}@daibx.test", Uuid::new_v4()),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    #[sqlx::test]
    async fn creates_a_setup_challenge_holding_a_sealed_new_secret(db_pool: PgPool) {
        let totp_keyring = test_keyring();
        let user_id = seed_user_id(&db_pool).await;

        let challenge_token = CreateAuthChallenge::new(&totp_keyring)
            .execute(&db_pool, user_id, AuthChallengePurpose::MfaSetup)
            .await
            .expect("creating should succeed");
        let active_challenge = FindActiveAuthChallenge::new()
            .execute(&db_pool, &challenge_token)
            .await
            .expect("lookup should succeed")
            .expect("the challenge should be active");

        assert_eq!(challenge_token.len(), 43);
        assert_eq!(active_challenge.user_id, user_id);
        assert_eq!(active_challenge.purpose, AuthChallengePurpose::MfaSetup);
        assert_eq!(active_challenge.attempts, 0);
        let sealed_secret = active_challenge
            .pending_totp_secret
            .expect("a setup challenge should hold a secret");
        let opened_secret = totp_keyring
            .open(user_id, &sealed_secret)
            .expect("the secret should open for its own user");
        assert_eq!(opened_secret.len(), TOTP_SECRET_BYTE_COUNT);
    }

    #[sqlx::test]
    async fn creates_a_verify_challenge_without_a_secret(db_pool: PgPool) {
        let totp_keyring = test_keyring();
        let user_id = seed_user_id(&db_pool).await;

        let challenge_token = CreateAuthChallenge::new(&totp_keyring)
            .execute(&db_pool, user_id, AuthChallengePurpose::MfaVerify)
            .await
            .expect("creating should succeed");

        let active_challenge = FindActiveAuthChallenge::new()
            .execute(&db_pool, &challenge_token)
            .await
            .expect("lookup should succeed")
            .expect("the challenge should be active");
        assert_eq!(active_challenge.pending_totp_secret, None);
    }
}
