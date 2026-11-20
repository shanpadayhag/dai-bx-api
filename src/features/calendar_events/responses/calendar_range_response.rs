use serde::Serialize;

use crate::features::calendar_events::entities::calendar_event::CalendarRange;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::card_schedules::responses::card_schedule_response::ScheduledMoveResponse;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Debug, Serialize)]
pub struct CalendarRangeResponse {
    pub events: Vec<CalendarEventResponse>,
    pub moves: Vec<ScheduledMoveResponse>,
}

impl CalendarRangeResponse {
    pub fn new(calendar_range: CalendarRange, uncheck_cutoffs: &UncheckCutoffs) -> Self {
        Self {
            events: calendar_range
                .events
                .into_iter()
                .map(|calendar_event| CalendarEventResponse::new(calendar_event, uncheck_cutoffs))
                .collect(),
            moves: calendar_range
                .moves
                .into_iter()
                .map(ScheduledMoveResponse::from)
                .collect(),
        }
    }
}
