use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::task_groups::actions::lock_task_group::LockTaskGroup;
use crate::features::tasks::data_sources::postgres::task_source::PostgresTaskSource;
use crate::features::tasks::entities::task::{GroupTasks, MAX_TASK_DEPTH, NewTask};
use crate::features::tasks::error::TaskError;

pub struct AddTask<'a> {
    db_pool: &'a PgPool,
    lock_task_group: LockTaskGroup,
    find_uncheck_cutoffs: FindUncheckCutoffs,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> AddTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_task_group: LockTaskGroup::new(),
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_group_id: Uuid,
        task_name: &str,
        parent_task_id: Option<Uuid>,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<GroupTasks, TaskError> {
        let mut add_transaction = self.db_pool.begin().await?;
        let task_group_details = self
            .lock_task_group
            .execute(&mut *add_transaction, user_id, task_group_id)
            .await?;
        if let Some(parent_task_id) = parent_task_id {
            let parent_depth = self
                .postgres_task_source
                .find_depth(&mut *add_transaction, task_group_id, parent_task_id)
                .await?
                .ok_or(TaskError::NotFound)?;
            if parent_depth >= MAX_TASK_DEPTH {
                return Err(TaskError::Invalid(String::from(
                    "tasks nest at most 32 levels deep",
                )));
            }
        }
        let next_position = self
            .postgres_task_source
            .next_position(&mut *add_transaction, task_group_id, parent_task_id)
            .await?;
        self.postgres_task_source
            .insert(
                &mut *add_transaction,
                &NewTask {
                    id: Uuid::now_v7(),
                    user_id,
                    group_id: task_group_id,
                    parent_id: parent_task_id,
                    position: next_position,
                    name: task_name.to_owned(),
                    card_id: None,
                },
            )
            .await?;
        let uncheck_cutoff_millis = self
            .find_uncheck_cutoffs
            .execute(&mut *add_transaction, user_id, request_time_zone, now)
            .await?
            .cutoff_millis_for(task_group_id);
        if let Some(parent_task_id) = parent_task_id {
            self.postgres_task_source
                .reopen_task_and_ancestors(
                    &mut *add_transaction,
                    task_group_id,
                    parent_task_id,
                    uncheck_cutoff_millis,
                )
                .await?;
        }
        let group_tasks = self
            .postgres_task_source
            .list_for_group(&mut *add_transaction, task_group_id, uncheck_cutoff_millis)
            .await?;
        add_transaction.commit().await?;
        Ok(GroupTasks {
            group: task_group_details,
            tasks: group_tasks,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::tasks::data_sources::postgres::task_source::tests::{
        seed_task_group, seed_user_id,
    };

    #[sqlx::test]
    async fn refuses_a_parent_in_another_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let first_group = seed_task_group(&db_pool, owner_id).await;
        let second_group = seed_task_group(&db_pool, owner_id).await;
        let first_group_tasks = AddTask::new(&db_pool)
            .execute(
                owner_id,
                first_group.id,
                "Parent",
                None,
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the task should be added");
        let parent_task_id = first_group_tasks.tasks[0].id;

        let cross_group_outcome = AddTask::new(&db_pool)
            .execute(
                owner_id,
                second_group.id,
                "Stray",
                Some(parent_task_id),
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await;

        assert!(matches!(cross_group_outcome, Err(TaskError::NotFound)));
    }
}
