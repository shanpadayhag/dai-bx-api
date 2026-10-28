use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::data_sources::postgres::category_source::PostgresCategorySource;
use crate::features::categories::entities::category::Category;
use crate::features::categories::error::CategoryError;

#[derive(Default)]
pub struct ListCategories {
    postgres_category_source: PostgresCategorySource,
}

impl ListCategories {
    pub fn new() -> Self {
        Self {
            postgres_category_source: PostgresCategorySource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<Category>, CategoryError> {
        Ok(self
            .postgres_category_source
            .list_for_user(executor, user_id)
            .await?)
    }
}
