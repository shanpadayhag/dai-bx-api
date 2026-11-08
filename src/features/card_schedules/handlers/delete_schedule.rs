use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::delete_card_schedule::DeleteCardSchedule;
use crate::features::card_schedules::responses::card_schedule_response::CardSchedulesResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_schedule(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(schedule_id_text): Path<String>,
) -> Result<Json<CardSchedulesResponse>, AppError> {
    let schedule_id = Uuid::parse_str(&schedule_id_text).map_err(|_| AppError::NotFound)?;
    let card_schedules = DeleteCardSchedule::new(&app_state.db_pool)
        .execute(write_session.user_id, schedule_id, UtcInstant::now())
        .await?;
    Ok(Json(card_schedules.into()))
}
