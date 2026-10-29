use serde::Serialize;

use crate::features::tasks::entities::task::{AffectedGroup, GroupTasks, MovedTask, Task};

#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub id: String,
    pub parent_id: Option<String>,
    pub position: i32,
    pub name: String,
    pub is_done: bool,
    pub estimate_minutes: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupTasksResponse {
    pub group: GroupResponse,
    pub tasks: Vec<TaskResponse>,
    pub open_count: usize,
    pub done_count: usize,
}

impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        Self {
            id: task.id.to_string(),
            parent_id: task.parent_id.map(|parent_id| parent_id.to_string()),
            position: task.position,
            name: task.name,
            is_done: task.is_done,
            estimate_minutes: task.estimate_minutes,
        }
    }
}

impl From<GroupTasks> for GroupTasksResponse {
    fn from(group_tasks: GroupTasks) -> Self {
        let open_count = group_tasks.open_count();
        let done_count = group_tasks.done_count();
        Self {
            group: GroupResponse {
                id: group_tasks.group.id.to_string(),
                name: group_tasks.group.name,
            },
            tasks: group_tasks
                .tasks
                .into_iter()
                .map(TaskResponse::from)
                .collect(),
            open_count,
            done_count,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedGroupResponse {
    pub id: String,
    pub open_task_count: i64,
    pub task_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedTaskResponse {
    #[serde(flatten)]
    pub group_tasks: GroupTasksResponse,
    pub affected_groups: Vec<AffectedGroupResponse>,
}

impl From<AffectedGroup> for AffectedGroupResponse {
    fn from(affected_group: AffectedGroup) -> Self {
        Self {
            id: affected_group.id.to_string(),
            open_task_count: affected_group.open_task_count,
            task_count: affected_group.task_count,
        }
    }
}

impl From<MovedTask> for MovedTaskResponse {
    fn from(moved_task: MovedTask) -> Self {
        Self {
            group_tasks: moved_task.left_group_tasks.into(),
            affected_groups: moved_task
                .affected_groups
                .into_iter()
                .map(AffectedGroupResponse::from)
                .collect(),
        }
    }
}
