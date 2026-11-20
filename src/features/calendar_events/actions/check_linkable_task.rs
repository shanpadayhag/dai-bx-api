use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Default)]
pub struct CheckLinkableTask {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl CheckLinkableTask {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<(), CalendarEventError> {
        match self
            .postgres_calendar_event_source
            .find_task_parent_id(executor, user_id, task_id)
            .await?
        {
            None => Err(CalendarEventError::NotFound),
            Some(Some(_)) => Err(CalendarEventError::Invalid(String::from(
                "only a top-level task can be linked to a planned event",
            ))),
            Some(None) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        seed_task_ids, seed_user_id,
    };
    use sqlx::PgPool;

    #[sqlx::test]
    async fn accepts_the_owner_s_top_level_task_and_refuses_the_rest(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, task_group_id) = seed_task_ids(&db_pool, user_id).await;
        let subtask_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
             VALUES ($1, $2, $3, $4, 0, 'Outline')",
        )
        .bind(subtask_id)
        .bind(user_id)
        .bind(task_group_id)
        .bind(task_id)
        .execute(&db_pool)
        .await
        .expect("the subtask should be inserted");
        let check_linkable_task = CheckLinkableTask::new();

        let top_level_result = check_linkable_task
            .execute(&db_pool, user_id, task_id)
            .await;
        let subtask_result = check_linkable_task
            .execute(&db_pool, user_id, subtask_id)
            .await;
        let foreign_result = check_linkable_task
            .execute(&db_pool, other_user_id, task_id)
            .await;

        assert!(top_level_result.is_ok());
        assert!(matches!(
            subtask_result,
            Err(CalendarEventError::Invalid(_))
        ));
        assert!(matches!(foreign_result, Err(CalendarEventError::NotFound)));
    }
}
