use sqlx::PgExecutor;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::auth_challenges::data_sources::postgres::auth_challenge_source::PostgresAuthChallengeSource;
use crate::features::auth_challenges::entities::auth_challenge::{
    AUTH_CHALLENGE_MAX_ATTEMPTS, AuthChallenge, AuthChallengePurpose,
};
use crate::features::auth_challenges::error::AuthChallengeError;

#[derive(Default)]
pub struct ClaimAuthChallengeAttempt {
    postgres_auth_challenge_source: PostgresAuthChallengeSource,
}

impl ClaimAuthChallengeAttempt {
    pub fn new() -> Self {
        Self {
            postgres_auth_challenge_source: PostgresAuthChallengeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        challenge_token: &str,
        purpose: AuthChallengePurpose,
    ) -> Result<Option<AuthChallenge>, AuthChallengeError> {
        Ok(self
            .postgres_auth_challenge_source
            .claim_attempt(
                executor,
                &hash_opaque_token(challenge_token),
                purpose,
                AUTH_CHALLENGE_MAX_ATTEMPTS,
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;
    use crate::core::security::totp_secret_cipher::TotpKeyring;
    use crate::features::auth_challenges::actions::create_auth_challenge::CreateAuthChallenge;
    use crate::features::auth_challenges::actions::delete_auth_challenge::DeleteAuthChallenge;
    use crate::features::auth_challenges::actions::find_active_auth_challenge::FindActiveAuthChallenge;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    async fn seed_setup_challenge(db_pool: &PgPool) -> String {
        let totp_keyring =
            TotpKeyring::parse("v1:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "v1")
                .expect("test keyring should parse");
        let user_id: Uuid = CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: String::from("person@example.com"),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id;
        CreateAuthChallenge::new(&totp_keyring)
            .execute(db_pool, user_id, AuthChallengePurpose::MfaSetup)
            .await
            .expect("creating the challenge should succeed")
            .to_string()
    }

    #[sqlx::test]
    async fn counts_each_attempt_and_stops_after_five(db_pool: PgPool) {
        let challenge_token = seed_setup_challenge(&db_pool).await;

        let mut claimed_attempts = Vec::new();
        for _ in 0..6 {
            let claimed_challenge = ClaimAuthChallengeAttempt::new()
                .execute(&db_pool, &challenge_token, AuthChallengePurpose::MfaSetup)
                .await
                .expect("claiming should run");
            claimed_attempts.push(claimed_challenge.map(|challenge| challenge.attempts));
        }

        assert_eq!(
            claimed_attempts,
            vec![Some(1), Some(2), Some(3), Some(4), Some(5), None]
        );
    }

    #[sqlx::test]
    async fn refuses_a_challenge_of_another_purpose(db_pool: PgPool) {
        let challenge_token = seed_setup_challenge(&db_pool).await;

        let claimed_challenge = ClaimAuthChallengeAttempt::new()
            .execute(&db_pool, &challenge_token, AuthChallengePurpose::MfaVerify)
            .await
            .expect("claiming should run");

        assert_eq!(claimed_challenge, None);
    }

    #[sqlx::test]
    async fn a_deleted_challenge_is_gone(db_pool: PgPool) {
        let challenge_token = seed_setup_challenge(&db_pool).await;
        let claimed_challenge = ClaimAuthChallengeAttempt::new()
            .execute(&db_pool, &challenge_token, AuthChallengePurpose::MfaSetup)
            .await
            .expect("claiming should run")
            .expect("the challenge should be claimable");

        DeleteAuthChallenge::new()
            .execute(&db_pool, claimed_challenge.id)
            .await
            .expect("deleting should succeed");

        assert_eq!(
            FindActiveAuthChallenge::new()
                .execute(&db_pool, &challenge_token)
                .await
                .expect("lookup should succeed"),
            None
        );
    }
}
