use sqlx::PgExecutor;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

pub struct IsEmailTaken {
    postgres_user_source: PostgresUserSource,
}

impl Default for IsEmailTaken {
    fn default() -> Self {
        Self::new()
    }
}

impl IsEmailTaken {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        email: &str,
    ) -> Result<bool, UserError> {
        Ok(self
            .postgres_user_source
            .find_by_email(executor, email.trim())
            .await?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::data_sources::postgres::user_source::tests::new_user_with_email;

    #[sqlx::test]
    async fn finds_a_taken_address_in_any_case_and_not_a_free_one(db_pool: PgPool) {
        CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run");
        let is_email_taken = IsEmailTaken::new();

        assert!(
            is_email_taken
                .execute(&db_pool, " Person@Example.com ")
                .await
                .expect("lookup should succeed")
        );
        assert!(
            !is_email_taken
                .execute(&db_pool, "free@example.com")
                .await
                .expect("lookup should succeed")
        );
    }
}
