use serde::Serialize;

use crate::features::planning::entities::up_next::{UpNext, UpNextBlock, UpNextTask};
use crate::features::tasks::responses::group_tasks_response::TaskDueResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpNextEventResponse {
    pub id: String,
    pub starts_at: String,
    pub ends_at: String,
}

impl From<UpNextBlock> for UpNextEventResponse {
    fn from(planned_block: UpNextBlock) -> Self {
        Self {
            id: planned_block.event_id.to_string(),
            starts_at: planned_block.starts_at.to_iso_text(),
            ends_at: planned_block.ends_at.to_iso_text(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpNextTaskResponse {
    pub task_id: String,
    pub name: String,
    pub group_id: String,
    pub group_name: String,
    pub estimate_minutes: Option<i64>,
    pub due: Option<TaskDueResponse>,
    pub priority: &'static str,
    pub planned_event: Option<UpNextEventResponse>,
    pub missed: bool,
}

impl From<UpNextTask> for UpNextTaskResponse {
    fn from(up_next_task: UpNextTask) -> Self {
        Self {
            task_id: up_next_task.task_id.to_string(),
            name: up_next_task.name,
            group_id: up_next_task.group_id.to_string(),
            group_name: up_next_task.group_name,
            estimate_minutes: up_next_task.estimate_minutes,
            due: up_next_task.due.map(TaskDueResponse::from),
            priority: up_next_task.priority.as_str(),
            planned_event: up_next_task.planned_block.map(UpNextEventResponse::from),
            missed: up_next_task.is_missed,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpNextResponse {
    pub tasks: Vec<UpNextTaskResponse>,
    pub refresh_at: String,
}

impl From<UpNext> for UpNextResponse {
    fn from(up_next: UpNext) -> Self {
        Self {
            tasks: up_next
                .tasks
                .into_iter()
                .map(UpNextTaskResponse::from)
                .collect(),
            refresh_at: up_next.refresh_at.to_iso_text(),
        }
    }
}
