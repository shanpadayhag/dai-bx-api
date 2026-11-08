use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::list_schedule_runs::ListScheduleRuns;
use crate::features::card_schedules::entities::schedule_run::RunPage;
use crate::features::card_schedules::responses::schedule_run_response::ScheduleRunResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetScheduleRunsQuery {
    pub before: Option<String>,
    pub limit: Option<String>,
}

pub async fn get_schedule_runs(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Path(schedule_id_text): Path<String>,
    Query(runs_query): Query<GetScheduleRunsQuery>,
) -> Result<Json<Vec<ScheduleRunResponse>>, AppError> {
    let schedule_id = Uuid::parse_str(&schedule_id_text).map_err(|_| AppError::NotFound)?;
    let run_page = RunPage::parse(runs_query.before.as_deref(), runs_query.limit.as_deref())?;
    let schedule_runs = ListScheduleRuns::new(&app_state.db_pool)
        .execute(
            current_session.user_id,
            schedule_id,
            run_page,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(schedule_runs.into_iter().map(Into::into).collect()))
}
