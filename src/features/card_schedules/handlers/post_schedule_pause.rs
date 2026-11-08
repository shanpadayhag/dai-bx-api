use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::set_schedule_paused::SetSchedulePaused;
use crate::features::card_schedules::responses::card_schedule_response::CardScheduleResponse;
use crate::features::throttling::entities::write_session::WriteSession;

async fn set_paused(
    app_state: &AppState,
    write_session: &WriteSession,
    schedule_id_text: &str,
    is_paused: bool,
) -> Result<Json<CardScheduleResponse>, AppError> {
    let schedule_id = Uuid::parse_str(schedule_id_text).map_err(|_| AppError::NotFound)?;
    let changed_schedule = SetSchedulePaused::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            schedule_id,
            is_paused,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(changed_schedule.into()))
}

pub async fn post_schedule_pause(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(schedule_id_text): Path<String>,
) -> Result<Json<CardScheduleResponse>, AppError> {
    set_paused(&app_state, &write_session, &schedule_id_text, true).await
}

pub async fn post_schedule_resume(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(schedule_id_text): Path<String>,
) -> Result<Json<CardScheduleResponse>, AppError> {
    set_paused(&app_state, &write_session, &schedule_id_text, false).await
}
