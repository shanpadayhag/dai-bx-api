use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct MarkEmailVerified {
    postgres_user_source: PostgresUserSource,
}

impl MarkEmailVerified {
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
        Ok(self
            .postgres_user_source
            .mark_email_verified(executor, user_id)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::actions::find_user_by_email::FindUserByEmail;
    use crate::features::users::data_sources::postgres::user_source::tests::new_user_with_email;

    #[sqlx::test]
    async fn marks_the_users_email_verified(db_pool: PgPool) {
        let created_user = CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run")
            .expect("the email should be free");

        MarkEmailVerified::new()
            .execute(&db_pool, created_user.id)
            .await
            .expect("marking should succeed");

        assert!(
            FindUserByEmail::new(&db_pool)
                .execute("person@example.com")
                .await
                .expect("lookup should succeed")
                .is_some_and(|found_user| found_user.is_email_verified)
        );
    }
}
