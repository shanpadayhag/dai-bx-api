use crate::core::recurrence::RepeatRule;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventSource, SeriesOccurrence,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesOccurrenceResponse {
    pub id: String,
    pub original_start: String,
    pub repeat: RepeatRule,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CalendarEventSourceResponse {
    #[serde(rename_all = "camelCase")]
    Task { id: String, group_id: String },
    #[serde(rename_all = "camelCase")]
    Card { id: String, board_id: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventResponse {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    pub category_id: Option<String>,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub finished_at: Option<String>,
    pub last_answered_at: Option<String>,
    pub source: Option<CalendarEventSourceResponse>,
    pub time_zone: Option<String>,
    pub series: Option<SeriesOccurrenceResponse>,
}

impl From<CalendarEvent> for CalendarEventResponse {
    fn from(calendar_event: CalendarEvent) -> Self {
        let event_id = match &calendar_event.series {
            Some(series_occurrence) => format!(
                "{}:{}",
                series_occurrence.series_id,
                series_occurrence.original_start.to_iso_text()
            ),
            None => calendar_event.id.to_string(),
        };
        Self {
            id: event_id,
            kind: calendar_event.kind.as_str(),
            title: calendar_event.title,
            category_id: calendar_event
                .category_id
                .map(|category_id| category_id.to_string()),
            starts_at: calendar_event.starts_at.to_iso_text(),
            ends_at: calendar_event.ends_at.map(|ends_at| ends_at.to_iso_text()),
            finished_at: calendar_event
                .finished_at
                .map(|finished_at| finished_at.to_iso_text()),
            last_answered_at: calendar_event
                .last_answered_at
                .map(|last_answered_at| last_answered_at.to_iso_text()),
            source: calendar_event
                .source
                .map(|event_source| match event_source {
                    CalendarEventSource::Task { task_id, group_id } => {
                        CalendarEventSourceResponse::Task {
                            id: task_id.to_string(),
                            group_id: group_id.to_string(),
                        }
                    }
                    CalendarEventSource::Card { card_id, board_id } => {
                        CalendarEventSourceResponse::Card {
                            id: card_id.to_string(),
                            board_id: board_id.to_string(),
                        }
                    }
                }),
            time_zone: calendar_event.time_zone,
            series: calendar_event
                .series
                .map(
                    |series_occurrence: SeriesOccurrence| SeriesOccurrenceResponse {
                        id: series_occurrence.series_id.to_string(),
                        original_start: series_occurrence.original_start.to_iso_text(),
                        repeat: series_occurrence.repeat_rule,
                    },
                ),
        }
    }
}
