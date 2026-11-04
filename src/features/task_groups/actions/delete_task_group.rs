use sqlx::PgPool;
use uuid::Uuid;

use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::task_groups::actions::lock_task_group::LockTaskGroup;
use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::error::TaskGroupError;

pub struct DeleteTaskGroup<'a> {
    db_pool: &'a PgPool,
    lock_task_group: LockTaskGroup,
    close_running_events: CloseRunningEvents,
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl<'a> DeleteTaskGroup<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_task_group: LockTaskGroup::new(),
            close_running_events: CloseRunningEvents::new(),
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, task_group_id: Uuid) -> Result<(), TaskGroupError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        self.lock_task_group
            .execute(&mut *delete_transaction, user_id, task_group_id)
            .await?;
        self.close_running_events
            .execute(
                &mut delete_transaction,
                user_id,
                ClosingScope::TaskGroup(task_group_id),
            )
            .await?;
        self.postgres_task_group_source
            .delete(&mut *delete_transaction, user_id, task_group_id)
            .await?;
        delete_transaction.commit().await?;
        Ok(())
    }
}
