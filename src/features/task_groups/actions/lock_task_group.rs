use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct LockTaskGroup {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl LockTaskGroup {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<TaskGroupDetails, TaskGroupError> {
        self.postgres_task_group_source
            .lock_details(executor, user_id, task_group_id)
            .await?
            .ok_or(TaskGroupError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::task_groups::data_sources::postgres::task_group_source::tests::{
        seed_task_group, seed_user_id,
    };

    #[sqlx::test]
    async fn locks_only_the_owner_s_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_group = seed_task_group(&db_pool, owner_id, "Mine").await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockTaskGroup::new()
            .execute(&mut *lock_transaction, owner_id, owner_group.id)
            .await
            .expect("the owner should lock their group");
        let foreign_lock = LockTaskGroup::new()
            .execute(&mut *lock_transaction, other_person_id, owner_group.id)
            .await;

        assert_eq!(owner_lock.name, "Mine");
        assert!(matches!(foreign_lock, Err(TaskGroupError::NotFound)));
    }
}
