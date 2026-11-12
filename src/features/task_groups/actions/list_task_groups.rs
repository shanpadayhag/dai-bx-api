use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::ScheduledTaskGroup;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct ListTaskGroups {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl ListTaskGroups {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
        only_task_group_id: Option<Uuid>,
    ) -> Result<Vec<ScheduledTaskGroup>, TaskGroupError> {
        let group_timetables = self
            .postgres_task_group_source
            .list_rules(&mut *connection, user_id, only_task_group_id)
            .await?
            .work_out_timetables(request_time_zone, now);
        let uncheck_cutoffs = UncheckCutoffs::from_timetables(
            group_timetables
                .iter()
                .map(|(task_group_id, _, group_timetable)| (*task_group_id, group_timetable)),
        );
        let mut scheduled_rules: HashMap<Uuid, _> = group_timetables
            .into_iter()
            .map(|(task_group_id, group_rules, group_timetable)| {
                (task_group_id, (group_rules, group_timetable))
            })
            .collect();
        let task_groups = self
            .postgres_task_group_source
            .list_for_user(
                &mut *connection,
                user_id,
                only_task_group_id,
                &uncheck_cutoffs,
            )
            .await?;
        Ok(task_groups
            .into_iter()
            .filter_map(|task_group| {
                scheduled_rules
                    .remove(&task_group.id)
                    .map(|(rules, timetable)| ScheduledTaskGroup {
                        task_group,
                        rules,
                        timetable,
                    })
            })
            .collect())
    }
}
