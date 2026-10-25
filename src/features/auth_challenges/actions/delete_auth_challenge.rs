use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::auth_challenges::data_sources::postgres::auth_challenge_source::PostgresAuthChallengeSource;
use crate::features::auth_challenges::error::AuthChallengeError;

#[derive(Default)]
pub struct DeleteAuthChallenge {
    postgres_auth_challenge_source: PostgresAuthChallengeSource,
}

impl DeleteAuthChallenge {
    pub fn new() -> Self {
        Self {
            postgres_auth_challenge_source: PostgresAuthChallengeSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        challenge_id: Uuid,
    ) -> Result<(), AuthChallengeError> {
        Ok(self
            .postgres_auth_challenge_source
            .delete(executor, challenge_id)
            .await?)
    }
}
