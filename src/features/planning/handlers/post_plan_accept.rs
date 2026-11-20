use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::find_linked_task_cutoffs::FindLinkedTaskCutoffs;
use crate::features::planning::actions::accept_plan::AcceptPlan;
use crate::features::planning::entities::plan_request::validate_accepted_count;
use crate::features::planning::handlers::plan_block_request::PlanBlockRequest;
use crate::features::planning::responses::accepted_plan_response::AcceptedPlanResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostPlanAcceptRequest {
    pub blocks: Vec<PlanBlockRequest>,
}

pub async fn post_plan_accept(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Json(accept_request): Json<PostPlanAcceptRequest>,
) -> Result<(StatusCode, Json<AcceptedPlanResponse>), AppError> {
    validate_accepted_count(accept_request.blocks.len())?;
    let accepted_blocks = accept_request
        .blocks
        .into_iter()
        .map(PlanBlockRequest::into_plan_block)
        .collect::<Result<Vec<_>, _>>()?;
    let created_events = AcceptPlan::new(&app_state.db_pool)
        .execute(write_session.user_id, &accepted_blocks, &request_time_zone)
        .await?;
    let uncheck_cutoffs = FindLinkedTaskCutoffs::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            &created_events,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(AcceptedPlanResponse::new(created_events, &uncheck_cutoffs)),
    ))
}
