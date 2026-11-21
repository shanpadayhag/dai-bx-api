use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::account::actions::find_time_tracking_settings::FindTimeTrackingSettings;
use crate::features::activity::actions::record_activity::RecordActivity;
use crate::features::activity::entities::activity_entry::{
    ActivityDetails, ActivitySubject, NewActivityEntry,
};
use crate::features::calendar_events::actions::create_task_blocks::{
    CreateTaskBlocks, TaskBlockPlacement,
};
use crate::features::calendar_events::entities::calendar_event::{CalendarEvent, TaskBlock};
use crate::features::planning::data_sources::postgres::planning_source::PostgresPlanningSource;
use crate::features::planning::entities::plan_request::PlanBlock;
use crate::features::planning::entities::planning_task::BlockTask;
use crate::features::planning::error::PlanningError;

pub struct AcceptPlan<'a> {
    db_pool: &'a PgPool,
    find_time_tracking_settings: FindTimeTrackingSettings,
    postgres_planning_source: PostgresPlanningSource,
    create_task_blocks: CreateTaskBlocks,
    record_activity: RecordActivity,
}

impl<'a> AcceptPlan<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_time_tracking_settings: FindTimeTrackingSettings::new(),
            postgres_planning_source: PostgresPlanningSource,
            create_task_blocks: CreateTaskBlocks::new(),
            record_activity: RecordActivity::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        accepted_blocks: &[PlanBlock],
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<Vec<CalendarEvent>, PlanningError> {
        let mut accept_transaction = self.db_pool.begin().await?;
        let time_tracking_settings = self
            .find_time_tracking_settings
            .execute(&mut *accept_transaction, user_id)
            .await?;
        let mut task_ids: Vec<Uuid> = accepted_blocks
            .iter()
            .map(|accepted_block| accepted_block.task_id)
            .collect();
        task_ids.sort_unstable();
        task_ids.dedup();
        let block_tasks: HashMap<Uuid, BlockTask> = self
            .postgres_planning_source
            .list_block_tasks(&mut *accept_transaction, user_id, &task_ids)
            .await?
            .into_iter()
            .map(|block_task| (block_task.id, block_task))
            .collect();
        let task_blocks = accepted_blocks
            .iter()
            .map(|accepted_block| {
                let block_task = block_tasks
                    .get(&accepted_block.task_id)
                    .ok_or(PlanningError::NotFound)?;
                if !block_task.is_top_level {
                    return Err(PlanningError::Invalid(String::from(
                        "only a top-level task can be linked to a planned event",
                    )));
                }
                Ok(TaskBlock {
                    task_id: block_task.id,
                    title: block_task.name.clone(),
                    starts_at: accepted_block.span.start,
                    ends_at: accepted_block.span.end,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let request_zone_name = request_time_zone
            .0
            .as_ref()
            .and_then(|time_zone| time_zone.iana_name());
        let created_events = self
            .create_task_blocks
            .execute(
                &mut accept_transaction,
                user_id,
                TaskBlockPlacement {
                    category_id: time_tracking_settings.tracked_category_id,
                    time_zone: time_tracking_settings
                        .calendar_time_zone
                        .as_deref()
                        .or(request_zone_name),
                },
                &task_blocks,
            )
            .await?;
        self.record_activity
            .execute(
                &mut *accept_transaction,
                NewActivityEntry {
                    user_id,
                    occurred_at: now,
                    subject: ActivitySubject::Nothing,
                    details: ActivityDetails::PlanAccepted {
                        block_count: created_events.len() as i64,
                    },
                },
            )
            .await?;
        accept_transaction.commit().await?;
        Ok(created_events)
    }
}
