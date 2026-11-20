use serde::Serialize;

use crate::core::wall_clock::{WallClockWindow, format_wall_clock_time};
use crate::features::tasks::entities::task::{
    AffectedGroup, GroupTasks, MovedTask, Task, TaskCard, TaskDue,
};

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
    pub card: Option<TaskCardResponse>,
    pub tracked_event_count: i64,
    pub priority: &'static str,
    pub due: Option<TaskDueResponse>,
    pub plan_between: Option<PlanBetweenResponse>,
}

#[derive(Debug, Serialize)]
pub struct TaskDueResponse {
    pub date: String,
    pub time: Option<String>,
}

impl From<TaskDue> for TaskDueResponse {
    fn from(task_due: TaskDue) -> Self {
        Self {
            date: task_due.date.to_string(),
            time: task_due.time.map(format_wall_clock_time),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlanBetweenResponse {
    pub start: String,
    pub end: String,
}

impl From<WallClockWindow> for PlanBetweenResponse {
    fn from(plan_window: WallClockWindow) -> Self {
        Self {
            start: format_wall_clock_time(plan_window.start),
            end: format_wall_clock_time(plan_window.end),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCardResponse {
    pub id: String,
    pub title: String,
    pub board_id: String,
}

impl From<TaskCard> for TaskCardResponse {
    fn from(task_card: TaskCard) -> Self {
        Self {
            id: task_card.id.to_string(),
            title: task_card.title,
            board_id: task_card.board_id.to_string(),
        }
    }
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
            card: task.card.map(TaskCardResponse::from),
            tracked_event_count: task.tracked_event_count,
            priority: task.planning.priority.as_str(),
            due: task.planning.due.map(TaskDueResponse::from),
            plan_between: task.planning.plan_window.map(PlanBetweenResponse::from),
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
