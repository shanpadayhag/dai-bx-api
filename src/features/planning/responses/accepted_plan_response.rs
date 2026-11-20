use serde::Serialize;

use crate::features::calendar_events::entities::calendar_event::CalendarEvent;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Debug, Serialize)]
pub struct AcceptedPlanResponse {
    pub events: Vec<CalendarEventResponse>,
}

impl AcceptedPlanResponse {
    pub fn new(created_events: Vec<CalendarEvent>, uncheck_cutoffs: &UncheckCutoffs) -> Self {
        Self {
            events: created_events
                .into_iter()
                .map(|created_event| CalendarEventResponse::new(created_event, uncheck_cutoffs))
                .collect(),
        }
    }
}
