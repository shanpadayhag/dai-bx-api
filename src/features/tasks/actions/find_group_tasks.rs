use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::find_task_group_details::FindTaskGroupDetails;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::GroupTasks;
use crate::features::tasks::error::TaskError;

pub struct FindGroupTasks<'a> {
    db_pool: &'a PgPool,
    find_task_group_details: FindTaskGroupDetails,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> FindGroupTasks<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_task_group_details: FindTaskGroupDetails::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_group_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let task_group_details = self
            .find_task_group_details
            .execute(self.db_pool, user_id, task_group_id)
            .await?;
        let uncheck_cutoff_millis = self
            .find_uncheck_cutoffs
            .execute(self.db_pool, user_id, request_time_zone, now)
            .await?
            .cutoff_millis_for(task_group_details.id);
        let group_tasks = self
            .postgres_task_source
            .list_for_group(self.db_pool, task_group_details.id, uncheck_cutoff_millis)
            .await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}
