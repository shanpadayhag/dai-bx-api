use serde::Serialize;

use crate::features::task_groups::entities::task_group::TaskGroup;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroupResponse {
    pub id: String,
    pub name: String,
    pub open_task_count: i64,
    pub task_count: i64,
}

impl From<TaskGroup> for TaskGroupResponse {
    fn from(task_group: TaskGroup) -> Self {
        Self {
            id: task_group.id.to_string(),
            name: task_group.name,
            open_task_count: task_group.open_task_count,
            task_count: task_group.task_count,
        }
    }
}
