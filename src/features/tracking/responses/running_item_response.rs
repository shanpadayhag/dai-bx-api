use serde::Serialize;

use crate::features::calendar_events::entities::calendar_event::RunningItem;
use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningItemResponse {
    #[serde(flatten)]
    pub event: CalendarEventResponse,
    pub estimate_minutes: Option<i32>,
}

impl From<RunningItem> for RunningItemResponse {
    fn from(running_item: RunningItem) -> Self {
        Self {
            event: running_item.event.into(),
            estimate_minutes: running_item.estimate_minutes,
        }
    }
}
