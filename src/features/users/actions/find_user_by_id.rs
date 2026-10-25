use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::users::data_sources::postgres::user_source::PostgresUserSource;
use crate::features::users::entities::user::User;
use crate::features::users::error::UserError;

#[derive(Default)]
pub struct FindUserById {
    postgres_user_source: PostgresUserSource,
}

impl FindUserById {
    pub fn new() -> Self {
        Self {
            postgres_user_source: PostgresUserSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<User>, UserError> {
        Ok(self
            .postgres_user_source
            .find_by_id(executor, user_id)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::data_sources::postgres::user_source::tests::new_user_with_email;

    #[sqlx::test]
    async fn finds_a_user_by_id_and_nothing_for_an_unknown_id(db_pool: PgPool) {
        let created_user = CreateUser::new()
            .execute(&db_pool, &new_user_with_email("person@example.com"))
            .await
            .expect("creating should run")
            .expect("the email should be free");

        assert_eq!(
            FindUserById::new()
                .execute(&db_pool, created_user.id)
                .await
                .expect("lookup should succeed"),
            Some(created_user)
        );
        assert_eq!(
            FindUserById::new()
                .execute(&db_pool, Uuid::now_v7())
                .await
                .expect("lookup should succeed"),
            None
        );
    }
}
