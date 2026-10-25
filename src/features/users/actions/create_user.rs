use sqlx::PgExecutor;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::entities::user::{NewUser, User};
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct CreateUser {
    postgres_user_source: PostgresUserSource,
}

impl CreateUser {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        new_user: &NewUser,
    ) -> Result<Option<User>, UserError> {
        Ok(self
            .postgres_user_source
            .insert_if_email_free(executor, new_user)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::data_sources::postgres::user_source::tests::new_user_with_email;

    #[sqlx::test]
    async fn creates_a_user_once_per_email(db_pool: PgPool) {
        let first_creation = CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run");
        let second_creation = CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run");

        assert!(first_creation.is_some());
        assert_eq!(second_creation, None);
    }
}
