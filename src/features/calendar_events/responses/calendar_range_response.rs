use serde::Serialize;

use crate::features::calendar_events::entities::calendar_event::CalendarRange;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::card_schedules::responses::card_schedule_response::ScheduledMoveResponse;

#[derive(Debug, Serialize)]
pub struct CalendarRangeResponse {
    pub events: Vec<CalendarEventResponse>,
    pub moves: Vec<ScheduledMoveResponse>,
}

impl From<CalendarRange> for CalendarRangeResponse {
    fn from(calendar_range: CalendarRange) -> Self {
        Self {
            events: calendar_range
                .events
                .into_iter()
                .map(CalendarEventResponse::from)
                .collect(),
            moves: calendar_range
                .moves
                .into_iter()
                .map(ScheduledMoveResponse::from)
                .collect(),
        }
    }
}
