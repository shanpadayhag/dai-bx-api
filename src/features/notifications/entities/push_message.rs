use std::time::Duration;

use serde_json::{Map, Value, json};
use uuid::Uuid;

const MAX_SHOWN_TITLE_CHARS: usize = 120;
const MAX_SHOWN_CONTEXT_CHARS: usize = 60;
const SHORT_LIVED_TTL: Duration = Duration::from_secs(600);
const SCHEDULE_RUN_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushKind {
    CheckIn,
    ScheduleRan,
    BlockStarting,
    Test,
}

impl PushKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PushKind::CheckIn => "check_in",
            PushKind::ScheduleRan => "schedule_ran",
            PushKind::BlockStarting => "block_starting",
            PushKind::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushUrgency {
    Normal,
    High,
}

impl PushUrgency {
    pub fn as_str(self) -> &'static str {
        match self {
            PushUrgency::Normal => "normal",
            PushUrgency::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushAction {
    pub action_name: &'static str,
    pub button_title: &'static str,
    pub answer_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMessage {
    pub kind: PushKind,
    pub tag: String,
    pub title: String,
    pub body: String,
    pub open_path: String,
    pub actions: Vec<PushAction>,
    pub default_action_name: Option<&'static str>,
    pub time_to_live: Duration,
    pub urgency: PushUrgency,
    pub topic: Option<String>,
}

pub struct ScheduleRunNotice<'a> {
    pub run_id: Uuid,
    pub card_id: Uuid,
    pub board_id: Uuid,
    pub card_title: &'a str,
    pub board_name: &'a str,
    pub moved_to_list_name: Option<&'a str>,
}

pub struct BlockStartNotice<'a> {
    pub subject_key: &'a str,
    pub event_title: &'a str,
    pub time_span_text: &'a str,
    pub linked_task_id: Option<Uuid>,
}

impl PushMessage {
    pub fn check_in(
        event_id: Uuid,
        item_title: &str,
        context_name: &str,
        minutes_so_far: i64,
    ) -> Self {
        Self {
            kind: PushKind::CheckIn,
            tag: format!("check-in:{event_id}"),
            title: format!("Still on {}?", shorten(item_title, MAX_SHOWN_TITLE_CHARS)),
            body: format!(
                "{} · {} so far",
                shorten(context_name, MAX_SHOWN_CONTEXT_CHARS),
                format_minutes(minutes_so_far)
            ),
            open_path: String::from("/today"),
            actions: vec![
                PushAction {
                    action_name: "check-in",
                    button_title: "Check in",
                    answer_path: format!("/tracking/{event_id}/check-in"),
                },
                PushAction {
                    action_name: "finish",
                    button_title: "Finish",
                    answer_path: format!("/tracking/{event_id}/finish"),
                },
            ],
            default_action_name: Some("check-in"),
            time_to_live: SHORT_LIVED_TTL,
            urgency: PushUrgency::High,
            topic: Some(event_id.simple().to_string()),
        }
    }

    pub fn schedule_ran(schedule_run_notice: &ScheduleRunNotice<'_>) -> Self {
        let shown_card_title = shorten(schedule_run_notice.card_title, MAX_SHOWN_TITLE_CHARS);
        let title = match schedule_run_notice.moved_to_list_name {
            Some(moved_to_list_name) => format!(
                "{shown_card_title} moved to {}",
                shorten(moved_to_list_name, MAX_SHOWN_CONTEXT_CHARS)
            ),
            None => format!("Schedule ran on {shown_card_title}"),
        };
        Self {
            kind: PushKind::ScheduleRan,
            tag: format!("schedule:{}", schedule_run_notice.run_id),
            title,
            body: shorten(schedule_run_notice.board_name, MAX_SHOWN_CONTEXT_CHARS),
            open_path: format!(
                "/board?board={}&card={}",
                schedule_run_notice.board_id, schedule_run_notice.card_id
            ),
            actions: vec![PushAction {
                action_name: "start-card",
                button_title: "Start card",
                answer_path: format!("/cards/{}/start", schedule_run_notice.card_id),
            }],
            default_action_name: None,
            time_to_live: SCHEDULE_RUN_TTL,
            urgency: PushUrgency::Normal,
            topic: None,
        }
    }

    pub fn block_starting(block_start_notice: &BlockStartNotice<'_>) -> Self {
        Self {
            kind: PushKind::BlockStarting,
            tag: format!("block:{}", block_start_notice.subject_key),
            title: format!(
                "{} starts now",
                shorten(block_start_notice.event_title, MAX_SHOWN_TITLE_CHARS)
            ),
            body: block_start_notice.time_span_text.to_owned(),
            open_path: String::from("/today"),
            actions: block_start_notice
                .linked_task_id
                .map(|linked_task_id| PushAction {
                    action_name: "start",
                    button_title: "Start",
                    answer_path: format!("/tasks/{linked_task_id}/start"),
                })
                .into_iter()
                .collect(),
            default_action_name: None,
            time_to_live: SHORT_LIVED_TTL,
            urgency: PushUrgency::Normal,
            topic: None,
        }
    }

    pub fn test() -> Self {
        Self {
            kind: PushKind::Test,
            tag: String::from("test"),
            title: String::from("Notifications are on"),
            body: String::from("This device will get DaiBX notifications."),
            open_path: String::from("/settings"),
            actions: Vec::new(),
            default_action_name: None,
            time_to_live: SHORT_LIVED_TTL,
            urgency: PushUrgency::Normal,
            topic: None,
        }
    }

    pub fn to_payload_bytes(&self) -> Vec<u8> {
        let mut answer_paths = Map::with_capacity(self.actions.len() + 1);
        if let Some(default_action_name) = self.default_action_name {
            answer_paths.insert(String::from("default"), json!(default_action_name));
        }
        for push_action in &self.actions {
            answer_paths.insert(
                push_action.action_name.to_owned(),
                json!({ "path": push_action.answer_path }),
            );
        }
        let shown_actions: Vec<Value> = self
            .actions
            .iter()
            .map(|push_action| {
                json!({ "action": push_action.action_name, "title": push_action.button_title })
            })
            .collect();
        json!({
            "kind": self.kind.as_str(),
            "tag": self.tag,
            "title": self.title,
            "body": self.body,
            "url": self.open_path,
            "actions": shown_actions,
            "answer": answer_paths,
        })
        .to_string()
        .into_bytes()
    }
}

pub fn format_minutes(total_minutes: i64) -> String {
    let whole_hours = total_minutes / 60;
    let leftover_minutes = total_minutes % 60;
    match (whole_hours, leftover_minutes) {
        (0, _) => format!("{leftover_minutes}m"),
        (_, 0) => format!("{whole_hours}h"),
        _ => format!("{whole_hours}h {leftover_minutes}m"),
    }
}

fn shorten(shown_text: &str, max_shown_chars: usize) -> String {
    match shown_text.char_indices().nth(max_shown_chars) {
        Some((cut_index, _)) => format!("{}…", &shown_text[..cut_index]),
        None => shown_text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_PAYLOAD_BYTES: usize = 3072;

    fn payload_json(push_message: &PushMessage) -> Value {
        serde_json::from_slice(&push_message.to_payload_bytes()).expect("payload should be JSON")
    }

    #[test]
    fn a_check_in_answers_with_check_in_by_default_and_offers_finish() {
        let event_id = Uuid::nil();

        let check_in_payload = payload_json(&PushMessage::check_in(
            event_id,
            "Write the spec",
            "DaiBX v3",
            80,
        ));

        assert_eq!(check_in_payload["title"], "Still on Write the spec?");
        assert_eq!(check_in_payload["body"], "DaiBX v3 · 1h 20m so far");
        assert_eq!(check_in_payload["answer"]["default"], "check-in");
        assert_eq!(
            check_in_payload["answer"]["check-in"]["path"],
            format!("/tracking/{event_id}/check-in")
        );
        assert_eq!(
            check_in_payload["answer"]["finish"]["path"],
            format!("/tracking/{event_id}/finish")
        );
        assert_eq!(check_in_payload["actions"][1]["title"], "Finish");
    }

    #[test]
    fn a_schedule_run_names_the_column_only_when_the_card_moved() {
        let schedule_run_notice = ScheduleRunNotice {
            run_id: Uuid::nil(),
            card_id: Uuid::nil(),
            board_id: Uuid::nil(),
            card_title: "Weekly review",
            board_name: "Routines",
            moved_to_list_name: Some("Doing"),
        };

        let moved_payload = payload_json(&PushMessage::schedule_ran(&schedule_run_notice));
        let unmoved_payload = payload_json(&PushMessage::schedule_ran(&ScheduleRunNotice {
            moved_to_list_name: None,
            ..schedule_run_notice
        }));

        assert_eq!(moved_payload["title"], "Weekly review moved to Doing");
        assert_eq!(unmoved_payload["title"], "Schedule ran on Weekly review");
        assert!(moved_payload["answer"].get("default").is_none());
    }

    #[test]
    fn a_block_offers_start_only_when_linked_to_a_task() {
        let linked_payload = payload_json(&PushMessage::block_starting(&BlockStartNotice {
            subject_key: "event",
            event_title: "Deep work",
            time_span_text: "16:00–18:00",
            linked_task_id: Some(Uuid::nil()),
        }));
        let unlinked_payload = payload_json(&PushMessage::block_starting(&BlockStartNotice {
            subject_key: "event",
            event_title: "Gym",
            time_span_text: "10:00–11:00",
            linked_task_id: None,
        }));

        assert_eq!(linked_payload["actions"][0]["action"], "start");
        assert_eq!(unlinked_payload["actions"], json!([]));
    }

    #[test]
    fn stays_under_the_payload_limit_with_the_longest_names() {
        let longest_title = "𝔄".repeat(500);
        let longest_context = "𝔄".repeat(200);

        let check_in_message =
            PushMessage::check_in(Uuid::nil(), &longest_title, &longest_context, 600);
        let schedule_message = PushMessage::schedule_ran(&ScheduleRunNotice {
            run_id: Uuid::nil(),
            card_id: Uuid::nil(),
            board_id: Uuid::nil(),
            card_title: &longest_title,
            board_name: &longest_context,
            moved_to_list_name: Some(&longest_context),
        });

        assert!(check_in_message.to_payload_bytes().len() < MAX_PAYLOAD_BYTES);
        assert!(schedule_message.to_payload_bytes().len() < MAX_PAYLOAD_BYTES);
    }

    #[test]
    fn formats_minutes_as_hours_and_minutes() {
        assert_eq!(format_minutes(45), "45m");
        assert_eq!(format_minutes(120), "2h");
        assert_eq!(format_minutes(135), "2h 15m");
    }
}
