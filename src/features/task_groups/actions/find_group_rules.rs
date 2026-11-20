use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::group_rules::GroupRulesListing;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct FindGroupRules {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl FindGroupRules {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<GroupRulesListing, TaskGroupError> {
        Ok(self
            .postgres_task_group_source
            .list_rules(executor, user_id, None)
            .await?)
    }
}
