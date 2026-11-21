use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::WallClockWindow;
use crate::features::calendar_events::entities::calendar_event::EventRange;
use crate::features::notifications::entities::check_in_due::CheckInSettings;
use crate::features::notifications::entities::notification_settings::NotificationSettings;
use crate::features::notifications::entities::push_message::PushKind;

pub const NOTIFICATION_WINDOW_MINUTES: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationWindow {
    pub after: UtcInstant,
    pub until: UtcInstant,
}

impl NotificationWindow {
    pub fn ending_at(now: UtcInstant) -> Self {
        Self {
            after: now.plus_minutes(-NOTIFICATION_WINDOW_MINUTES),
            until: now,
        }
    }

    pub fn contains(&self, instant: UtcInstant) -> bool {
        self.after < instant && instant <= self.until
    }

    pub fn as_event_range(&self) -> EventRange {
        EventRange {
            from: UtcInstant::from_unix_millis(self.after.unix_millis() + 1),
            to: UtcInstant::from_unix_millis(self.until.unix_millis() + 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecipient {
    pub user_id: Uuid,
    pub notification_settings: NotificationSettings,
    pub check_in_settings: CheckInSettings,
    pub working_hours: Option<WallClockWindow>,
    pub calendar_time_zone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueCheckIn {
    pub event_id: Uuid,
    pub item_title: String,
    pub context_name: Option<String>,
    pub started_at: UtcInstant,
    pub due_at: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueScheduleRun {
    pub run_id: Uuid,
    pub card_id: Uuid,
    pub board_id: Uuid,
    pub card_title: String,
    pub board_name: String,
    pub moved_to_list_name: Option<String>,
    pub ran_at: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueBlockStart {
    pub subject_key: String,
    pub event_title: String,
    pub starts_at: UtcInstant,
    pub ends_at: Option<UtcInstant>,
    pub linked_task_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DueItem {
    CheckIn(DueCheckIn),
    ScheduleRun(DueScheduleRun),
    BlockStart(DueBlockStart),
}

impl DueItem {
    pub fn kind(&self) -> PushKind {
        match self {
            DueItem::CheckIn(_) => PushKind::CheckIn,
            DueItem::ScheduleRun(_) => PushKind::ScheduleRan,
            DueItem::BlockStart(_) => PushKind::BlockStarting,
        }
    }

    pub fn subject_key(&self) -> String {
        match self {
            DueItem::CheckIn(due_check_in) => due_check_in.event_id.to_string(),
            DueItem::ScheduleRun(due_schedule_run) => due_schedule_run.run_id.to_string(),
            DueItem::BlockStart(due_block_start) => due_block_start.subject_key.clone(),
        }
    }

    pub fn due_at(&self) -> UtcInstant {
        match self {
            DueItem::CheckIn(due_check_in) => due_check_in.due_at,
            DueItem::ScheduleRun(due_schedule_run) => due_schedule_run.ran_at,
            DueItem::BlockStart(due_block_start) => due_block_start.starts_at,
        }
    }

    pub fn check_in_started_at(&self) -> Option<UtcInstant> {
        match self {
            DueItem::CheckIn(due_check_in) => Some(due_check_in.started_at),
            DueItem::ScheduleRun(_) | DueItem::BlockStart(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientDueItems {
    pub recipient: NotificationRecipient,
    pub due_items: Vec<DueItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("test instants are valid")
    }

    #[test]
    fn the_window_holds_the_last_five_minutes_up_to_now() {
        let notification_window = NotificationWindow::ending_at(instant("2026-11-22T10:00:00Z"));

        assert!(!notification_window.contains(instant("2026-11-22T09:55:00Z")));
        assert!(notification_window.contains(instant("2026-11-22T09:55:00.001Z")));
        assert!(notification_window.contains(instant("2026-11-22T10:00:00Z")));
        assert!(!notification_window.contains(instant("2026-11-22T10:00:00.001Z")));
    }
}
