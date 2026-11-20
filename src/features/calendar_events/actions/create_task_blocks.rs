use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::calendar_events::data_sources::postgres::calendar_event_source::PostgresCalendarEventSource;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, EventWrite, TaskBlock, validate_planned_times,
};
use crate::features::calendar_events::error::CalendarEventError;

#[derive(Debug, Clone, Copy)]
pub struct TaskBlockPlacement<'a> {
    pub category_id: Option<Uuid>,
    pub time_zone: Option<&'a str>,
}

#[derive(Default)]
pub struct CreateTaskBlocks {
    postgres_calendar_event_source: PostgresCalendarEventSource,
}

impl CreateTaskBlocks {
    pub fn new() -> Self {
        Self {
            postgres_calendar_event_source: PostgresCalendarEventSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        task_block_placement: TaskBlockPlacement<'_>,
        task_blocks: &[TaskBlock],
    ) -> Result<Vec<CalendarEvent>, CalendarEventError> {
        for task_block in task_blocks {
            validate_planned_times(task_block.starts_at, task_block.ends_at)?;
        }
        let block_ids: Vec<Uuid> = task_blocks.iter().map(|_| Uuid::now_v7()).collect();
        let event_write = self
            .postgres_calendar_event_source
            .insert_task_blocks(
                &mut *connection,
                user_id,
                task_block_placement.category_id,
                task_block_placement.time_zone,
                &block_ids,
                task_blocks,
            )
            .await?;
        if event_write == EventWrite::UnknownLink {
            return Err(CalendarEventError::NotFound);
        }
        Ok(self
            .postgres_calendar_event_source
            .list_by_ids(&mut *connection, user_id, &block_ids)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::calendar_events::data_sources::postgres::calendar_event_source::tests::{
        instant, seed_category_id, seed_task_ids, seed_user_id,
    };

    #[sqlx::test]
    async fn writes_every_block_as_a_linked_planned_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let task_blocks = vec![
            TaskBlock {
                task_id,
                title: String::from("Write the changelog"),
                starts_at: instant("2026-11-05T09:00:00Z"),
                ends_at: instant("2026-11-05T09:30:00Z"),
            },
            TaskBlock {
                task_id,
                title: String::from("Write the changelog"),
                starts_at: instant("2026-11-06T09:00:00Z"),
                ends_at: instant("2026-11-06T10:00:00Z"),
            },
        ];
        let mut connection = db_pool.acquire().await.expect("a connection");

        let created_events = CreateTaskBlocks::new()
            .execute(
                &mut connection,
                user_id,
                TaskBlockPlacement {
                    category_id: Some(category_id),
                    time_zone: Some("Asia/Manila"),
                },
                &task_blocks,
            )
            .await
            .expect("the blocks should be written");

        assert_eq!(created_events.len(), 2);
        for (created_event, task_block) in created_events.iter().zip(&task_blocks) {
            assert_eq!(created_event.task_id(), Some(task_id));
            assert_eq!(created_event.title, task_block.title);
            assert_eq!(created_event.starts_at, task_block.starts_at);
            assert_eq!(created_event.ends_at, Some(task_block.ends_at));
            assert_eq!(created_event.category_id, Some(category_id));
            assert_eq!(created_event.time_zone.as_deref(), Some("Asia/Manila"));
            assert!(created_event.linked_task.is_some());
        }
    }

    #[sqlx::test]
    async fn refuses_a_block_under_a_quarter_hour(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let mut connection = db_pool.acquire().await.expect("a connection");

        let create_result = CreateTaskBlocks::new()
            .execute(
                &mut connection,
                user_id,
                TaskBlockPlacement {
                    category_id: None,
                    time_zone: None,
                },
                &[TaskBlock {
                    task_id,
                    title: String::from("Write the changelog"),
                    starts_at: instant("2026-11-05T09:00:00Z"),
                    ends_at: instant("2026-11-05T09:10:00Z"),
                }],
            )
            .await;

        assert!(matches!(create_result, Err(CalendarEventError::Invalid(_))));
    }
}
