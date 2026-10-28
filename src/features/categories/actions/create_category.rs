use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::data_sources::postgres::category_source::PostgresCategorySource;
use crate::features::categories::entities::category::{Category, CategorySave, NewCategory};
use crate::features::categories::error::CategoryError;

#[derive(Default)]
pub struct CreateCategory {
    postgres_category_source: PostgresCategorySource,
}

impl CreateCategory {
    pub fn new() -> Self {
        Self {
            postgres_category_source: PostgresCategorySource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        category_name: &str,
        category_color: &str,
    ) -> Result<Category, CategoryError> {
        let new_category = NewCategory {
            id: Uuid::now_v7(),
            user_id,
            name: category_name.to_owned(),
            color: category_color.to_owned(),
        };
        match self
            .postgres_category_source
            .insert(executor, &new_category)
            .await?
        {
            CategorySave::Saved(created_category) => Ok(created_category),
            CategorySave::NameTaken => Err(CategoryError::NameTaken),
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::categories::data_sources::postgres::category_source::tests::seed_user_id;

    #[sqlx::test]
    async fn creates_a_category_and_refuses_the_same_name_twice(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let create_category = CreateCategory::new();

        let created_category = create_category
            .execute(&db_pool, owner_id, "Health", "#8fae9a")
            .await
            .expect("the first Health should be created");
        let duplicate_outcome = create_category
            .execute(&db_pool, owner_id, "health", "#000000")
            .await;

        assert_eq!(created_category.name, "Health");
        assert!(matches!(duplicate_outcome, Err(CategoryError::NameTaken)));
    }
}
