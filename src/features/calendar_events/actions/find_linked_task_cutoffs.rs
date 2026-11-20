use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::calendar_event::CalendarEvent;
use crate::features::calendar_events::error::CalendarEventError;
use crate::features::task_groups::actions::find_uncheck_cutoffs::FindUncheckCutoffs;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

pub struct FindLinkedTaskCutoffs<'a> {
    db_pool: &'a PgPool,
    find_uncheck_cutoffs: FindUncheckCutoffs,
}

impl<'a> FindLinkedTaskCutoffs<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_uncheck_cutoffs: FindUncheckCutoffs::new(),
        }
    }

    pub async fn execute<'e>(
        &self,
        user_id: Uuid,
        calendar_events: impl IntoIterator<Item = &'e CalendarEvent>,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<UncheckCutoffs, CalendarEventError> {
        let has_linked_task = calendar_events
            .into_iter()
            .any(|calendar_event| calendar_event.linked_task.is_some());
        if !has_linked_task {
            return Ok(UncheckCutoffs::default());
        }
        Ok(self
            .find_uncheck_cutoffs
            .execute(self.db_pool, user_id, request_time_zone, now)
            .await?)
    }
}
