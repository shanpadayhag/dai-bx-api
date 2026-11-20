use serde::Serialize;

use crate::features::planning::actions::propose_plan::PlanProposal;
use crate::features::planning::entities::planner::{PlannedBlock, UnplacedTask};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedBlockResponse {
    pub task_id: String,
    pub task_name: String,
    pub group_id: String,
    pub group_name: String,
    pub starts_at: String,
    pub ends_at: String,
    pub due: Option<String>,
    pub late: bool,
}

impl From<PlannedBlock> for ProposedBlockResponse {
    fn from(planned_block: PlannedBlock) -> Self {
        Self {
            task_id: planned_block.task_id.to_string(),
            task_name: planned_block.task_name,
            group_id: planned_block.group_id.to_string(),
            group_name: planned_block.group_name,
            starts_at: planned_block.starts_at.to_iso_text(),
            ends_at: planned_block.ends_at.to_iso_text(),
            due: planned_block.due.map(|due| due.to_iso_text()),
            late: planned_block.is_late,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnplacedTaskResponse {
    pub task_id: String,
    pub task_name: String,
    pub group_name: String,
    pub reason: &'static str,
}

impl From<UnplacedTask> for UnplacedTaskResponse {
    fn from(unplaced_task: UnplacedTask) -> Self {
        Self {
            task_id: unplaced_task.task_id.to_string(),
            task_name: unplaced_task.task_name,
            group_name: unplaced_task.group_name,
            reason: unplaced_task.reason.as_str(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProposalResponse {
    pub plan_start: String,
    pub range_end: String,
    pub blocks: Vec<ProposedBlockResponse>,
    pub unplaced: Vec<UnplacedTaskResponse>,
}

impl From<PlanProposal> for PlanProposalResponse {
    fn from(plan_proposal: PlanProposal) -> Self {
        Self {
            plan_start: plan_proposal.plan_range.plan_start.to_iso_text(),
            range_end: plan_proposal.plan_range.range_end.to_iso_text(),
            blocks: plan_proposal
                .plan
                .blocks
                .into_iter()
                .map(ProposedBlockResponse::from)
                .collect(),
            unplaced: plan_proposal
                .plan
                .unplaced
                .into_iter()
                .map(UnplacedTaskResponse::from)
                .collect(),
        }
    }
}
