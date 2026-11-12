use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::list_task_groups::ListTaskGroups;
use crate::features::task_groups::actions::lock_task_group::LockTaskGroup;
use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::group_rules::{GroupRuleChanges, today_in};
use crate::features::task_groups::entities::task_group::ScheduledTaskGroup;
use crate::features::task_groups::error::TaskGroupError;

pub struct UpdateTaskGroup<'a> {
    db_pool: &'a PgPool,
    lock_task_group: LockTaskGroup,
    list_task_groups: ListTaskGroups,
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl<'a> UpdateTaskGroup<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_task_group: LockTaskGroup::new(),
            list_task_groups: ListTaskGroups::new(),
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_group_id: Uuid,
        task_group_name: &str,
        group_rule_changes: &GroupRuleChanges,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<ScheduledTaskGroup, TaskGroupError> {
        let mut update_transaction = self.db_pool.begin().await?;
        self.lock_task_group
            .execute(&mut *update_transaction, user_id, task_group_id)
            .await?;
        let rules_listing = self
            .postgres_task_group_source
            .list_rules(&mut *update_transaction, user_id, Some(task_group_id))
            .await?;
        let calendar_time_zone =
            request_time_zone.calendar_time_zone(rules_listing.calendar_time_zone_name.as_deref());
        let today = today_in(&calendar_time_zone, now)?;
        let current_rules = rules_listing
            .group_rules
            .into_iter()
            .next()
            .map(|(_, group_rules)| group_rules)
            .ok_or(TaskGroupError::NotFound)?;
        self.postgres_task_group_source
            .update(
                &mut *update_transaction,
                user_id,
                task_group_id,
                task_group_name,
                &group_rule_changes.apply_to(&current_rules, today),
            )
            .await?;
        let updated_task_group = self
            .list_task_groups
            .execute(
                &mut update_transaction,
                user_id,
                request_time_zone,
                now,
                Some(task_group_id),
            )
            .await?
            .pop()
            .ok_or(TaskGroupError::NotFound)?;
        update_transaction.commit().await?;
        Ok(updated_task_group)
    }
}
