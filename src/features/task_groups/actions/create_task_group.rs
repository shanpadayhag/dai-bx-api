use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::data_sources::postgres::task_group_source::PostgresTaskGroupSource;
use crate::features::task_groups::entities::task_group::{NewTaskGroup, TaskGroup, TaskGroupSave};
use crate::features::task_groups::error::TaskGroupError;

#[derive(Default)]
pub struct CreateTaskGroup {
    postgres_task_group_source: PostgresTaskGroupSource,
}

impl CreateTaskGroup {
    pub fn new() -> Self {
        Self {
            postgres_task_group_source: PostgresTaskGroupSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_name: &str,
        category_id: Option<Uuid>,
    ) -> Result<TaskGroup, TaskGroupError> {
        let new_task_group = NewTaskGroup {
            id: Uuid::now_v7(),
            user_id,
            name: task_group_name.to_owned(),
            category_id,
        };
        match self
            .postgres_task_group_source
            .insert(executor, &new_task_group)
            .await?
        {
            TaskGroupSave::Saved(created_task_group) => Ok(created_task_group),
            TaskGroupSave::CategoryMissing => Err(TaskGroupError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::task_groups::data_sources::postgres::task_group_source::tests::seed_user_id;

    #[sqlx::test]
    async fn creates_a_group_and_answers_not_found_for_an_unknown_category(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let create_task_group = CreateTaskGroup::new();

        let created_task_group = create_task_group
            .execute(&db_pool, owner_id, "Today", None)
            .await
            .expect("the group should be created");
        let unknown_category_outcome = create_task_group
            .execute(&db_pool, owner_id, "Today", Some(Uuid::now_v7()))
            .await;

        assert_eq!(created_task_group.name, "Today");
        assert!(matches!(
            unknown_category_outcome,
            Err(TaskGroupError::NotFound)
        ));
    }
}
