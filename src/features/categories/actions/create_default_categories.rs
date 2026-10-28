use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::data_sources::postgres::category_source::PostgresCategorySource;
use crate::features::categories::entities::category::{DEFAULT_CATEGORIES, NewCategory};
use crate::features::categories::error::CategoryError;

#[derive(Default)]
pub struct CreateDefaultCategories {
    postgres_category_source: PostgresCategorySource,
}

impl CreateDefaultCategories {
    pub fn new() -> Self {
        Self {
            postgres_category_source: PostgresCategorySource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<(), CategoryError> {
        let default_categories: Vec<NewCategory> = DEFAULT_CATEGORIES
            .iter()
            .map(|(default_name, default_color)| NewCategory {
                id: Uuid::now_v7(),
                user_id,
                name: (*default_name).to_owned(),
                color: (*default_color).to_owned(),
            })
            .collect();
        Ok(self
            .postgres_category_source
            .insert_many(executor, &default_categories)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::categories::actions::list_categories::ListCategories;
    use crate::features::categories::data_sources::postgres::category_source::tests::seed_user_id;

    #[sqlx::test]
    async fn creates_the_four_default_categories_in_order(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;

        CreateDefaultCategories::new()
            .execute(&db_pool, owner_id)
            .await
            .expect("defaults should be created");

        let created_categories: Vec<(String, String)> = ListCategories::new()
            .execute(&db_pool, owner_id)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|category| (category.name, category.color))
            .collect();
        let expected_categories: Vec<(String, String)> = DEFAULT_CATEGORIES
            .iter()
            .map(|(default_name, default_color)| {
                ((*default_name).to_owned(), (*default_color).to_owned())
            })
            .collect();
        assert_eq!(created_categories, expected_categories);
    }

    #[test]
    fn matches_the_categories_the_backfill_migration_gives_existing_accounts() {
        let backfill_sql =
            include_str!("../../../../migrations/20261028090100_backfill_default_categories.sql");

        for (default_name, default_color) in DEFAULT_CATEGORIES {
            assert!(
                backfill_sql.contains(&format!("('{default_name}', '{default_color}')")),
                "the backfill should list {default_name} {default_color}"
            );
        }
    }
}
