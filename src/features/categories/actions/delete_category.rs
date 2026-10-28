use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::data_sources::postgres::category_source::PostgresCategorySource;
use crate::features::categories::error::CategoryError;

#[derive(Default)]
pub struct DeleteCategory {
    postgres_category_source: PostgresCategorySource,
}

impl DeleteCategory {
    pub fn new() -> Self {
        Self {
            postgres_category_source: PostgresCategorySource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        category_id: Uuid,
    ) -> Result<(), CategoryError> {
        if self
            .postgres_category_source
            .delete(executor, user_id, category_id)
            .await?
        {
            Ok(())
        } else {
            Err(CategoryError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::categories::actions::create_category::CreateCategory;
    use crate::features::categories::data_sources::postgres::category_source::tests::seed_user_id;

    #[sqlx::test]
    async fn deletes_once_then_answers_not_found(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let health_category = CreateCategory::new()
            .execute(&db_pool, owner_id, "Health", "#8fae9a")
            .await
            .expect("Health should be created");
        let delete_category = DeleteCategory::new();

        let first_delete = delete_category
            .execute(&db_pool, owner_id, health_category.id)
            .await;
        let second_delete = delete_category
            .execute(&db_pool, owner_id, health_category.id)
            .await;

        assert!(first_delete.is_ok());
        assert!(matches!(second_delete, Err(CategoryError::NotFound)));
    }
}
