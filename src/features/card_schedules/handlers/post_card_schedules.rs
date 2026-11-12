use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::recurrence::RepeatRule;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::create_card_schedule::CreateCardSchedule;
use crate::features::card_schedules::entities::card_schedule::{
    ScheduleBlock, ScheduleKind, ScheduleSettings, ScheduleTiming,
};
use crate::features::card_schedules::responses::card_schedule_response::CardSchedulesResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleBlockRequest {
    minutes: i64,
    category_id: Option<String>,
    time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRequest {
    kind: Option<String>,
    target_list_id: Option<String>,
    start: Option<String>,
    input_zone: Option<String>,
    time_zone: Option<String>,
    repeat: Option<RepeatRule>,
    block: Option<ScheduleBlockRequest>,
}

fn parse_id(id_text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id_text).map_err(|_| AppError::NotFound)
}

fn required<'a>(field_value: Option<&'a str>, field_name: &str) -> Result<&'a str, AppError> {
    field_value.ok_or_else(|| AppError::Validation(format!("{field_name} is needed")))
}

impl ScheduleRequest {
    pub fn into_settings(self) -> Result<ScheduleSettings, AppError> {
        let kind = ScheduleKind::parse(required(self.kind.as_deref(), "kind")?)?;
        let timing = ScheduleTiming::from_input(
            kind,
            required(self.start.as_deref(), "start")?,
            required(self.input_zone.as_deref(), "inputZone")?,
            self.time_zone.as_deref(),
            self.repeat,
        )?;
        let block = self
            .block
            .map(|block_request| {
                let category_id = block_request
                    .category_id
                    .as_deref()
                    .map(parse_id)
                    .transpose()?;
                Ok::<_, AppError>(ScheduleBlock::new(
                    block_request.minutes,
                    category_id,
                    block_request.time.as_deref(),
                )?)
            })
            .transpose()?;
        Ok(ScheduleSettings {
            target_list_id: parse_id(required(self.target_list_id.as_deref(), "targetListId")?)?,
            timing,
            block,
        })
    }
}

pub async fn post_card_schedules(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
    Json(schedule_request): Json<ScheduleRequest>,
) -> Result<(StatusCode, Json<CardSchedulesResponse>), AppError> {
    let card_id = parse_id(&card_id_text)?;
    let schedule_settings = schedule_request.into_settings()?;
    let card_schedules = CreateCardSchedule::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            card_id,
            &schedule_settings,
            UtcInstant::now(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(card_schedules.into())))
}
