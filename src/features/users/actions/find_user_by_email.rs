use sqlx::PgPool;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::entities::user::User;
use crate::features::users::error::UserError;

pub struct FindUserByEmail<'a> {
    db_pool: &'a PgPool,
    postgres_user_source: PostgresUserSource,
}

impl<'a> FindUserByEmail<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(&self, email: &str) -> Result<Option<User>, UserError> {
        Ok(self
            .postgres_user_source
            .find_by_email(self.db_pool, email.trim())
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::data_sources::postgres::user_source::tests::new_user_with_email;

    #[sqlx::test]
    async fn finds_a_user_by_a_padded_email_in_any_case(db_pool: PgPool) {
        CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run");

        let found_user = FindUserByEmail::new(&db_pool)
            .execute("  PERSON@example.com ")
            .await
            .expect("lookup should succeed");

        assert!(found_user.is_some());
    }
}
