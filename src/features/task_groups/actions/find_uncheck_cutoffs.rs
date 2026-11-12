use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct FindUncheckCutoffs {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl FindUncheckCutoffs {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<UncheckCutoffs, TaskGroupError> {
        let group_timetables = self
            .postgres_task_group_source
            .list_rules(executor, user_id, None)
            .await?
            .work_out_timetables(request_time_zone, now);
        Ok(UncheckCutoffs::from_timetables(
            group_timetables
                .iter()
                .map(|(task_group_id, _, group_timetable)| (*task_group_id, group_timetable)),
        ))
    }
}
