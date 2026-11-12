use serde::Serialize;

use crate::core::recurrence::RepeatRule;
use crate::features::card_schedules::entities::card_schedule::{
    CardSchedule, ScheduleBlock, ScheduledMove,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleBlockResponse {
    pub minutes: i32,
    pub category_id: Option<String>,
    pub time: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardScheduleResponse {
    pub id: String,
    pub kind: &'static str,
    pub target_list_id: Option<String>,
    pub target_list_name: Option<String>,
    pub start: String,
    pub starts_at: Option<String>,
    pub time_zone: String,
    pub repeat: Option<RepeatRule>,
    pub block: Option<ScheduleBlockResponse>,
    pub paused: bool,
    pub next_run_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CardSchedulesResponse {
    pub schedules: Vec<CardScheduleResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledMoveResponse {
    pub schedule_id: String,
    pub card_id: String,
    pub card_title: String,
    pub board_id: String,
    pub list_name: String,
    pub at: String,
}

impl From<ScheduleBlock> for ScheduleBlockResponse {
    fn from(schedule_block: ScheduleBlock) -> Self {
        Self {
            minutes: schedule_block.minutes,
            category_id: schedule_block
                .category_id
                .map(|category_id| category_id.to_string()),
            time: schedule_block
                .start_time
                .map(|start_time| start_time.strftime("%H:%M").to_string()),
        }
    }
}

impl From<CardSchedule> for CardScheduleResponse {
    fn from(card_schedule: CardSchedule) -> Self {
        Self {
            id: card_schedule.id.to_string(),
            kind: card_schedule.timing.kind().as_str(),
            target_list_id: card_schedule
                .target_list_id
                .map(|target_list_id| target_list_id.to_string()),
            target_list_name: card_schedule.target_list_name,
            start: card_schedule
                .timing
                .local_start
                .strftime("%Y-%m-%dT%H:%M")
                .to_string(),
            starts_at: card_schedule
                .timing
                .start_instant()
                .map(|start_instant| start_instant.to_iso_text()),
            time_zone: card_schedule.timing.time_zone,
            repeat: card_schedule.timing.repeat_rule,
            block: card_schedule.block.map(ScheduleBlockResponse::from),
            paused: card_schedule.is_paused,
            next_run_at: card_schedule
                .next_run_at
                .map(|next_run_at| next_run_at.to_iso_text()),
        }
    }
}

impl From<Vec<CardSchedule>> for CardSchedulesResponse {
    fn from(card_schedules: Vec<CardSchedule>) -> Self {
        Self {
            schedules: card_schedules
                .into_iter()
                .map(CardScheduleResponse::from)
                .collect(),
        }
    }
}

impl From<ScheduledMove> for ScheduledMoveResponse {
    fn from(scheduled_move: ScheduledMove) -> Self {
        Self {
            schedule_id: scheduled_move.schedule_id.to_string(),
            card_id: scheduled_move.card_id.to_string(),
            card_title: scheduled_move.card_title,
            board_id: scheduled_move.board_id.to_string(),
            list_name: scheduled_move.list_name,
            at: scheduled_move.at.to_iso_text(),
        }
    }
}
