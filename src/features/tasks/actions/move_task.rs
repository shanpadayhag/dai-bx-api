use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::features::task_groups::actions::lock_task_group::LockTaskGroup;
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::tasks::data_sources::postgres::task_source::{PostgresTaskSource, TaskPlace};
use crate::features::tasks::entities::task::{
    AffectedGroup, GroupTasks, MAX_TASK_DEPTH, MovedTask,
};
use crate::features::tasks::error::TaskError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveTarget {
    pub group_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: i32,
}

pub struct MoveTask<'a> {
    db_pool: &'a PgPool,
    lock_task_group: LockTaskGroup,
    postgres_task_source: PostgresTaskSource,
}

impl<'a> MoveTask<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_task_group: LockTaskGroup::new(),
            postgres_task_source: PostgresTaskSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        task_id: Uuid,
        move_target: MoveTarget,
    ) -> Result<MovedTask, TaskError> {
        let mut move_transaction = self.db_pool.begin().await?;
        let source_group_id = self
            .postgres_task_source
            .find_group_id(&mut *move_transaction, user_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        let source_group_details = self
            .lock_groups_in_id_order(
                &mut move_transaction,
                user_id,
                source_group_id,
                move_target.group_id,
            )
            .await?;
        let source_place = self
            .postgres_task_source
            .find_place(&mut *move_transaction, source_group_id, task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        if let Some(parent_task_id) = move_target.parent_id {
            self.check_new_parent(
                &mut move_transaction,
                source_group_id,
                task_id,
                move_target.group_id,
                parent_task_id,
            )
            .await?;
        }
        self.postgres_task_source
            .close_position_gap(&mut *move_transaction, source_group_id, source_place)
            .await?;
        let sibling_count = self
            .postgres_task_source
            .count_siblings_except(
                &mut *move_transaction,
                move_target.group_id,
                move_target.parent_id,
                task_id,
            )
            .await?;
        let target_place = TaskPlace {
            parent_id: move_target.parent_id,
            position: move_target.position.min(sibling_count),
        };
        self.postgres_task_source
            .open_position_gap(
                &mut *move_transaction,
                move_target.group_id,
                target_place,
                task_id,
            )
            .await?;
        self.postgres_task_source
            .place(
                &mut *move_transaction,
                source_group_id,
                task_id,
                move_target.group_id,
                target_place,
            )
            .await?;
        if let Some(parent_task_id) = move_target.parent_id
            && self
                .postgres_task_source
                .subtree_has_open_task(&mut *move_transaction, move_target.group_id, task_id)
                .await?
        {
            self.postgres_task_source
                .reopen_task_and_ancestors(
                    &mut *move_transaction,
                    move_target.group_id,
                    parent_task_id,
                )
                .await?;
        }
        let left_group_tasks = GroupTasks {
            group: source_group_details,
            tasks: self
                .postgres_task_source
                .list_for_group(&mut *move_transaction, source_group_id)
                .await?,
        };
        let affected_groups = if move_target.group_id == source_group_id {
            Vec::new()
        } else {
            let destination_count = self
                .postgres_task_source
                .count_for_group(&mut *move_transaction, move_target.group_id)
                .await?;
            vec![
                left_group_tasks.affected_group(),
                AffectedGroup {
                    id: move_target.group_id,
                    open_task_count: destination_count.open_task_count,
                    task_count: destination_count.task_count,
                },
            ]
        };
        move_transaction.commit().await?;
        Ok(MovedTask {
            left_group_tasks,
            affected_groups,
        })
    }

    async fn lock_groups_in_id_order(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        source_group_id: Uuid,
        destination_group_id: Uuid,
    ) -> Result<TaskGroupDetails, TaskError> {
        if source_group_id == destination_group_id {
            return Ok(self
                .lock_task_group
                .execute(&mut *connection, user_id, source_group_id)
                .await?);
        }
        let (first_group_id, second_group_id) = if source_group_id < destination_group_id {
            (source_group_id, destination_group_id)
        } else {
            (destination_group_id, source_group_id)
        };
        let first_group_details = self
            .lock_task_group
            .execute(&mut *connection, user_id, first_group_id)
            .await?;
        let second_group_details = self
            .lock_task_group
            .execute(&mut *connection, user_id, second_group_id)
            .await?;
        Ok(if first_group_details.id == source_group_id {
            first_group_details
        } else {
            second_group_details
        })
    }

    async fn check_new_parent(
        &self,
        connection: &mut PgConnection,
        source_group_id: Uuid,
        task_id: Uuid,
        destination_group_id: Uuid,
        parent_task_id: Uuid,
    ) -> Result<(), TaskError> {
        if destination_group_id == source_group_id
            && self
                .postgres_task_source
                .subtree_contains(&mut *connection, source_group_id, task_id, parent_task_id)
                .await?
        {
            return Err(TaskError::Invalid(String::from(
                "a task can't move under itself or one of its own subtasks",
            )));
        }
        let parent_depth = self
            .postgres_task_source
            .find_depth(&mut *connection, destination_group_id, parent_task_id)
            .await?
            .ok_or(TaskError::NotFound)?;
        let subtree_height = self
            .postgres_task_source
            .subtree_height(&mut *connection, source_group_id, task_id)
            .await?;
        if parent_depth + subtree_height > MAX_TASK_DEPTH {
            return Err(TaskError::Invalid(String::from(
                "tasks nest at most 32 levels deep",
            )));
        }
        Ok(())
    }
}
