use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::find_event_task_id::FindEventTaskId;
use crate::features::calendar_events::actions::finish_running_event::FinishRunningEvent;
use crate::features::tasks::actions::complete_task_in_transaction::CompleteTaskInTransaction;
use crate::features::tasks::actions::lock_group_of_task::LockGroupOfTask;
use crate::features::tracking::entities::finished_item::FinishedItem;
use crate::features::tracking::error::TrackingError;

pub struct FinishRunningItem<'a> {
    db_pool: &'a PgPool,
    find_event_task_id: FindEventTaskId,
    lock_group_of_task: LockGroupOfTask,
    finish_running_event: FinishRunningEvent,
    complete_task_in_transaction: CompleteTaskInTransaction,
}

impl<'a> FinishRunningItem<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_event_task_id: FindEventTaskId::new(),
            lock_group_of_task: LockGroupOfTask::new(),
            finish_running_event: FinishRunningEvent::new(),
            complete_task_in_transaction: CompleteTaskInTransaction::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        event_id: Uuid,
        requested_ends_at: UtcInstant,
        marks_done: bool,
    ) -> Result<FinishedItem, TrackingError> {
        let mut finish_transaction = self.db_pool.begin().await?;
        let marked_task_id = if marks_done {
            let task_id = self
                .find_event_task_id
                .execute(&mut finish_transaction, user_id, event_id)
                .await?
                .ok_or_else(|| {
                    TrackingError::Invalid(String::from("only a task can be marked done"))
                })?;
            self.lock_group_of_task
                .execute(&mut finish_transaction, user_id, task_id)
                .await?;
            Some(task_id)
        } else {
            None
        };
        let finished_event = self
            .finish_running_event
            .execute(
                &mut finish_transaction,
                user_id,
                event_id,
                requested_ends_at,
            )
            .await?;
        let group_tasks = match marked_task_id {
            Some(task_id) => Some(
                self.complete_task_in_transaction
                    .execute(&mut finish_transaction, user_id, task_id)
                    .await?,
            ),
            None => None,
        };
        finish_transaction.commit().await?;
        Ok(FinishedItem {
            event: finished_event,
            group_tasks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tracking::actions::start_task::StartTask;
    use crate::features::tracking::actions::start_task::tests::{
        seed_task_and_subtask, seed_user_id,
    };

    #[sqlx::test]
    async fn marking_it_done_finishes_the_event_and_completes_the_task(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, subtask_id) = seed_task_and_subtask(&db_pool, owner_id).await;
        let running_item = StartTask::new(&db_pool)
            .execute(owner_id, task_id)
            .await
            .expect("the task should start");

        let finished_item = FinishRunningItem::new(&db_pool)
            .execute(
                owner_id,
                running_item.event.id,
                UtcInstant::now().plus_minutes(1),
                true,
            )
            .await
            .expect("the item should finish");

        assert!(!finished_item.event.is_running());
        let group_tasks = finished_item
            .group_tasks
            .expect("the group should come back");
        let done_task_ids: Vec<_> = group_tasks
            .tasks
            .iter()
            .filter(|task| task.is_done)
            .map(|task| task.id)
            .collect();
        assert_eq!(done_task_ids.len(), 2);
        assert!(done_task_ids.contains(&task_id) && done_task_ids.contains(&subtask_id));
    }

    #[sqlx::test]
    async fn stopping_leaves_the_task_open(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_and_subtask(&db_pool, owner_id).await;
        let running_item = StartTask::new(&db_pool)
            .execute(owner_id, task_id)
            .await
            .expect("the task should start");

        let finished_item = FinishRunningItem::new(&db_pool)
            .execute(owner_id, running_item.event.id, UtcInstant::now(), false)
            .await
            .expect("the item should finish");

        assert!(!finished_item.event.is_running());
        assert_eq!(finished_item.group_tasks, None);
    }
}
