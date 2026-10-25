use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct UpdatePasswordHash {
    postgres_user_source: PostgresUserSource,
}

impl UpdatePasswordHash {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<(), UserError> {
        Ok(self
            .postgres_user_source
            .update_password_hash(executor, user_id, password_hash)
            .await?)
    }
}
