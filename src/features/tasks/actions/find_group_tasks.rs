use sqlx::PgPool;
use uuid::Uuid;

use crate::features::task_groups::actions::find_task_group_details::FindTaskGroupDetails;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct FindGroupTasks<'a> {
    db_pool: &'a PgPool,
    find_task_group_details: FindTaskGroupDetails,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> FindGroupTasks<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_task_group_details: FindTaskGroupDetails::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<GroupTasks, TaskError> {
        let task_group_details = self
            .find_task_group_details
            .execute(self.db_pool, user_id, task_group_id)
            .await?;
        let group_tasks = self
            .postgres_task_source
            .list_for_group(self.db_pool, task_group_details.id)
            .await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
