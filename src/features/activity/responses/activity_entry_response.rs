use serde::Serialize;
use serde_json::Value;

use crate::features::activity::entities::activity_entry::ActivityEntry;
use crate::features::activity::entities::activity_page::ActivityPage;
use crate::features::activity::error::ActivityError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntryResponse {
    pub id: String,
    pub kind: String,
    pub occurred_at: String,
    pub title: Option<String>,
    pub context_name: Option<String>,
    pub task_id: Option<String>,
    pub card_id: Option<String>,
    pub board_id: Option<String>,
    pub details: Value,
}

impl TryFrom<ActivityEntry> for ActivityEntryResponse {
    type Error = ActivityError;

    fn try_from(activity_entry: ActivityEntry) -> Result<Self, Self::Error> {
        let (kind, details) = activity_entry.details.into_stored()?;
        Ok(Self {
            id: activity_entry.id.to_string(),
            kind,
            occurred_at: activity_entry.occurred_at.to_iso_text(),
            title: activity_entry.title,
            context_name: activity_entry.context_name,
            task_id: activity_entry.task_id.map(|task_id| task_id.to_string()),
            card_id: activity_entry.card_id.map(|card_id| card_id.to_string()),
            board_id: activity_entry.board_id.map(|board_id| board_id.to_string()),
            details,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPageResponse {
    pub entries: Vec<ActivityEntryResponse>,
    pub next_cursor: Option<String>,
}

impl TryFrom<ActivityPage> for ActivityPageResponse {
    type Error = ActivityError;

    fn try_from(activity_page: ActivityPage) -> Result<Self, Self::Error> {
        Ok(Self {
            entries: activity_page
                .entries
                .into_iter()
                .map(ActivityEntryResponse::try_from)
                .collect::<Result<_, _>>()?,
            next_cursor: activity_page.next_cursor.map(|cursor| cursor.to_text()),
        })
    }
}
