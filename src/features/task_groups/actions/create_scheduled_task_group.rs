use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::list_task_groups::ListTaskGroups;
use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::group_rules::{GroupRuleChanges, GroupRules, today_in};
use crate::features::task_groups::entities::task_group::{NewTaskGroup, ScheduledTaskGroup};
use crate::features::task_groups::error::TaskGroupError;

pub struct CreateScheduledTaskGroup<'a> {
    db_pool: &'a PgPool,
    list_task_groups: ListTaskGroups,
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl<'a> CreateScheduledTaskGroup<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            list_task_groups: ListTaskGroups::new(),
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_group_name: &str,
        group_rule_changes: &GroupRuleChanges,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<ScheduledTaskGroup, TaskGroupError> {
        let mut create_transaction = self.db_pool.begin().await?;
        let calendar_time_zone_name = self
            .postgres_task_group_source
            .find_calendar_time_zone_name(&mut *create_transaction, user_id)
            .await?;
        let calendar_time_zone =
            request_time_zone.calendar_time_zone(calendar_time_zone_name.as_deref());
        let today = today_in(&calendar_time_zone, now)?;
        let new_task_group = NewTaskGroup {
            id: Uuid::now_v7(),
            user_id,
            name: task_group_name.to_owned(),
            rules: group_rule_changes.apply_to(&GroupRules::default(), today),
        };
        self.postgres_task_group_source
            .insert(&mut *create_transaction, &new_task_group)
            .await?;
        let created_task_group = self
            .list_task_groups
            .execute(
                &mut create_transaction,
                user_id,
                request_time_zone,
                now,
                Some(new_task_group.id),
            )
            .await?
            .pop()
            .ok_or(TaskGroupError::NotFound)?;
        create_transaction.commit().await?;
        Ok(created_task_group)
    }
}
