use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::features::calendar_events::entities::calendar_event::parse_event_instant;
use crate::features::planning::entities::plan_request::{PlanBlock, validate_plan_block};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanBlockRequest {
    pub task_id: String,
    pub starts_at: String,
    pub ends_at: String,
}

pub fn parse_task_id(task_id_text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(task_id_text).map_err(|_| AppError::NotFound)
}

impl PlanBlockRequest {
    pub fn into_plan_block(self) -> Result<PlanBlock, AppError> {
        Ok(validate_plan_block(
            parse_task_id(&self.task_id)?,
            parse_event_instant(&self.starts_at, "startsAt")?,
            parse_event_instant(&self.ends_at, "endsAt")?,
        )?)
    }
}
