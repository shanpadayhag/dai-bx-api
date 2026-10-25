use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct DeleteUser {
    postgres_user_source: PostgresUserSource,
}

impl DeleteUser {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), UserError> {
        Ok(self.postgres_user_source.delete(executor, user_id).await?)
    }
}
