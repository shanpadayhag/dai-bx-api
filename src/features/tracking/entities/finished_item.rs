use crate::features::calendar_events::entities::calendar_event::CalendarEvent;
use crate::features::tasks::entities::task::GroupTasks;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedItem {
    pub event: CalendarEvent,
    pub group_tasks: Option<GroupTasks>,
}
