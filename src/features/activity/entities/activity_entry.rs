use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedSubject {
    Task,
    Card,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "details",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ActivityDetails {
    Started {
        subject: TrackedSubject,
        event_id: String,
    },
    Finished {
        subject: TrackedSubject,
        event_id: String,
        tracked_minutes: i64,
        marked_done: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ended_at_millis: Option<i64>,
    },
    CheckedIn {
        subject: TrackedSubject,
        event_id: String,
    },
    TaskCompleted {
        is_subtask: bool,
    },
    TaskReopened {
        is_subtask: bool,
    },
    CardMoved {
        from_list_name: Option<String>,
        to_list_name: Option<String>,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_title: Option<String>,
    },
    ScheduleRan {
        how: String,
        moved: bool,
        from_list_name: Option<String>,
        to_list_name: Option<String>,
    },
    PlanAccepted {
        block_count: i64,
    },
}

const ENDED_EARLY_MILLIS: i64 = 60_000;

impl ActivityDetails {
    pub fn finished(
        subject: TrackedSubject,
        event_id: Uuid,
        starts_at: UtcInstant,
        ends_at: UtcInstant,
        finished_at: UtcInstant,
        marked_done: bool,
    ) -> Self {
        let tracked_millis = ends_at.unix_millis() - starts_at.unix_millis();
        let is_ended_early =
            finished_at.unix_millis() - ends_at.unix_millis() >= ENDED_EARLY_MILLIS;
        Self::Finished {
            subject,
            event_id: event_id.to_string(),
            tracked_minutes: tracked_millis.max(0) / 60_000,
            marked_done,
            ended_at_millis: is_ended_early.then_some(ends_at.unix_millis()),
        }
    }

    pub fn into_stored(self) -> Result<(String, Value), serde_json::Error> {
        let Value::Object(mut tagged_fields) = serde_json::to_value(self)? else {
            return Err(serde::ser::Error::custom("activity details are an object"));
        };
        let kind = match tagged_fields.remove("kind") {
            Some(Value::String(kind)) => kind,
            _ => return Err(serde::ser::Error::custom("activity details carry a kind")),
        };
        let details = tagged_fields
            .remove("details")
            .unwrap_or_else(|| Value::Object(Map::new()));
        Ok((kind, details))
    }

    pub fn from_stored(kind: &str, details: Value) -> Result<Self, serde_json::Error> {
        let mut tagged_fields = Map::with_capacity(2);
        tagged_fields.insert(String::from("kind"), Value::String(kind.to_owned()));
        tagged_fields.insert(String::from("details"), details);
        serde_json::from_value(Value::Object(tagged_fields))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySubject {
    Task(Uuid),
    Card(Uuid),
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewActivityEntry {
    pub user_id: Uuid,
    pub occurred_at: UtcInstant,
    pub subject: ActivitySubject,
    pub details: ActivityDetails,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    pub id: Uuid,
    pub occurred_at: UtcInstant,
    pub title: Option<String>,
    pub context_name: Option<String>,
    pub task_id: Option<Uuid>,
    pub card_id: Option<Uuid>,
    pub board_id: Option<Uuid>,
    pub details: ActivityDetails,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_kind() -> Vec<ActivityDetails> {
        let event_id = Uuid::now_v7().to_string();
        vec![
            ActivityDetails::Started {
                subject: TrackedSubject::Task,
                event_id: event_id.clone(),
            },
            ActivityDetails::Finished {
                subject: TrackedSubject::Card,
                event_id: event_id.clone(),
                tracked_minutes: 80,
                marked_done: false,
                ended_at_millis: Some(1_000),
            },
            ActivityDetails::CheckedIn {
                subject: TrackedSubject::Task,
                event_id,
            },
            ActivityDetails::TaskCompleted { is_subtask: true },
            ActivityDetails::TaskReopened { is_subtask: false },
            ActivityDetails::CardMoved {
                from_list_name: Some(String::from("To Do")),
                to_list_name: None,
                reason: String::from("archived_idle"),
                task_title: None,
            },
            ActivityDetails::ScheduleRan {
                how: String::from("late"),
                moved: true,
                from_list_name: Some(String::from("To Do")),
                to_list_name: Some(String::from("Doing")),
            },
            ActivityDetails::PlanAccepted { block_count: 4 },
        ]
    }

    #[test]
    fn reads_back_every_kind_it_stores() {
        for details in every_kind() {
            let (kind, stored_details) =
                details.clone().into_stored().expect("details should store");

            let read_details =
                ActivityDetails::from_stored(&kind, stored_details).expect("details should read");

            assert_eq!(read_details, details);
        }
    }

    #[test]
    fn stores_the_kind_apart_from_camel_case_details() {
        let (kind, stored_details) = ActivityDetails::TaskCompleted { is_subtask: true }
            .into_stored()
            .expect("details should store");

        assert_eq!(kind, "task_completed");
        assert_eq!(stored_details, serde_json::json!({ "isSubtask": true }));
    }

    #[test]
    fn counts_whole_tracked_minutes_and_notes_an_early_end() {
        let starts_at = UtcInstant::from_unix_millis(0);
        let ends_at = starts_at.plus_minutes(80);

        let ended_early = ActivityDetails::finished(
            TrackedSubject::Task,
            Uuid::nil(),
            starts_at,
            ends_at,
            ends_at.plus_minutes(25),
            true,
        );
        let ended_now = ActivityDetails::finished(
            TrackedSubject::Task,
            Uuid::nil(),
            starts_at,
            ends_at,
            ends_at,
            true,
        );

        assert!(matches!(
            ended_early,
            ActivityDetails::Finished { tracked_minutes: 80, ended_at_millis: Some(millis), .. }
                if millis == ends_at.unix_millis()
        ));
        assert!(matches!(
            ended_now,
            ActivityDetails::Finished {
                ended_at_millis: None,
                ..
            }
        ));
    }

    #[test]
    fn refuses_an_unknown_stored_kind() {
        assert!(ActivityDetails::from_stored("renamed", serde_json::json!({})).is_err());
    }
}
