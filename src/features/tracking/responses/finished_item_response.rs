use serde::Serialize;

use crate::features::calendar_events::responses::calendar_event_response::CalendarEventResponse;
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;
use crate::features::tracking::entities::finished_item::FinishedItem;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedItemResponse {
    pub event: CalendarEventResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_tasks: Option<GroupTasksResponse>,
}

impl From<FinishedItem> for FinishedItemResponse {
    fn from(finished_item: FinishedItem) -> Self {
        Self {
            event: finished_item.event.into(),
            group_tasks: finished_item.group_tasks.map(GroupTasksResponse::from),
        }
    }
}
