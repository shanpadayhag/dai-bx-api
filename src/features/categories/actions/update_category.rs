use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::data_sources::postgres::category_source::PostgresCategorySource;
use crate::features::categories::entities::category::{Category, CategoryChanges, CategorySave};
use crate::features::categories::error::CategoryError;

#[derive(Default)]
pub struct UpdateCategory {
    postgres_category_source: PostgresCategorySource,
}

impl UpdateCategory {
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
        category_changes: &CategoryChanges,
    ) -> Result<Category, CategoryError> {
        match self
            .postgres_category_source
            .update(executor, user_id, category_id, category_changes)
            .await?
        {
            Some(CategorySave::Saved(updated_category)) => Ok(updated_category),
            Some(CategorySave::NameTaken) => Err(CategoryError::NameTaken),
            None => Err(CategoryError::NotFound),
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
    async fn answers_not_found_for_another_person_s_category(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let health_category = CreateCategory::new()
            .execute(&db_pool, owner_id, "Health", "#8fae9a")
            .await
            .expect("Health should be created");

        let update_outcome = UpdateCategory::new()
            .execute(
                &db_pool,
                other_person_id,
                health_category.id,
                &CategoryChanges {
                    name: Some(String::from("Mine")),
                    color: None,
                },
            )
            .await;

        assert!(matches!(update_outcome, Err(CategoryError::NotFound)));
    }
}
