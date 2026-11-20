use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::planning::actions::propose_plan::ProposePlan;
use crate::features::planning::entities::plan_request::{
    PlanRequest, validate_day_count, validate_pinned_count,
};
use crate::features::planning::handlers::plan_block_request::{PlanBlockRequest, parse_task_id};
use crate::features::planning::responses::plan_proposal_response::PlanProposalResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostPlanProposalRequest {
    pub days: i64,
    pub group_ids: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_task_ids: Vec<String>,
    #[serde(default)]
    pub pinned: Vec<PlanBlockRequest>,
}

fn parse_group_id(group_id_text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(group_id_text)
        .map_err(|_| AppError::Validation(String::from("a group in groupIds isn't one of yours")))
}

pub async fn post_plan_proposal(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Json(proposal_request): Json<PostPlanProposalRequest>,
) -> Result<Json<PlanProposalResponse>, AppError> {
    validate_pinned_count(proposal_request.pinned.len())?;
    let plan_request = PlanRequest {
        day_count: validate_day_count(proposal_request.days)?,
        group_ids: proposal_request
            .group_ids
            .map(|group_ids| {
                group_ids
                    .iter()
                    .map(|group_id_text| parse_group_id(group_id_text))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?,
        excluded_task_ids: proposal_request
            .exclude_task_ids
            .iter()
            .map(|task_id_text| parse_task_id(task_id_text))
            .collect::<Result<Vec<_>, _>>()?,
        pinned_blocks: proposal_request
            .pinned
            .into_iter()
            .map(PlanBlockRequest::into_plan_block)
            .collect::<Result<Vec<_>, _>>()?,
    };
    let plan_proposal = ProposePlan::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            &plan_request,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(plan_proposal.into()))
}
