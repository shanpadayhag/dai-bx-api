use sqlx::PgExecutor;

use crate::core::security::opaque_token::hash_opaque_token;
use crate::features::auth_challenges::data_sources::postgres::auth_challenge_source::PostgresAuthChallengeSource;
use crate::features::auth_challenges::entities::auth_challenge::AuthChallenge;
use crate::features::auth_challenges::error::AuthChallengeError;

#[derive(Default)]
pub struct FindActiveAuthChallenge {
    postgres_auth_challenge_source: PostgresAuthChallengeSource,
}

impl FindActiveAuthChallenge {
    pub fn new() -> Self {
        Self {
            postgres_auth_challenge_source: PostgresAuthChallengeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        challenge_token: &str,
    ) -> Result<Option<AuthChallenge>, AuthChallengeError> {
        Ok(self
            .postgres_auth_challenge_source
            .find_active(executor, &hash_opaque_token(challenge_token))
            .await?)
    }
}
