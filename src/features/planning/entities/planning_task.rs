use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::tasks::entities::task::{TaskDue, TaskPlanning};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningTask {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub group_id: Uuid,
    pub group_name: String,
    pub group_created_at: UtcInstant,
    pub position: i32,
    pub name: String,
    pub estimate_minutes: Option<i32>,
    pub completed_at: Option<UtcInstant>,
    pub planning: TaskPlanning,
    pub card_due: Option<TaskDue>,
    pub has_planned_block: bool,
    pub is_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTask {
    pub id: Uuid,
    pub name: String,
    pub is_top_level: bool,
}
