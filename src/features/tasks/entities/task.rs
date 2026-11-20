use jiff::civil::{Date, Time};
use uuid::Uuid;

use crate::core::recurrence::parse_calendar_date;
use crate::core::wall_clock::{WallClockWindow, parse_wall_clock_time};
use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::tasks::error::TaskError;

pub const TASK_NAME_MAX_CHARACTERS: usize = 500;
pub const MAX_TASK_DEPTH: i32 = 32;
pub const ESTIMATE_MINUTES_RANGE: std::ops::RangeInclusive<i64> = 1..=1440;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCard {
    pub id: Uuid,
    pub title: String,
    pub board_id: Uuid,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    High,
    #[default]
    Normal,
    Low,
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPriority::High => "high",
            TaskPriority::Normal => "normal",
            TaskPriority::Low => "low",
        }
    }

    pub fn parse(priority_text: &str) -> Option<Self> {
        match priority_text {
            "high" => Some(TaskPriority::High),
            "normal" => Some(TaskPriority::Normal),
            "low" => Some(TaskPriority::Low),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDue {
    pub date: Date,
    pub time: Option<Time>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskPlanning {
    pub priority: TaskPriority,
    pub due: Option<TaskDue>,
    pub plan_window: Option<WallClockWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub name: String,
    pub is_done: bool,
    pub estimate_minutes: Option<i32>,
    pub card: Option<TaskCard>,
    pub tracked_event_count: i64,
    pub planning: TaskPlanning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackableTask {
    pub name: String,
    pub card_id: Option<Uuid>,
    pub is_top_level: bool,
    pub is_done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedLinkedTask {
    pub task_id: Uuid,
    pub card_id: Uuid,
    pub name: String,
}

impl TrackableTask {
    pub fn can_start(&self) -> bool {
        self.is_top_level && !self.is_done
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub id: Uuid,
    pub user_id: Uuid,
    pub group_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub name: String,
    pub card_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskChanges {
    pub name: Option<String>,
    pub estimate_change: Option<Option<i32>>,
    pub priority: Option<TaskPriority>,
    pub due_change: Option<Option<TaskDue>>,
    pub plan_window_change: Option<Option<WallClockWindow>>,
}

impl TaskChanges {
    pub fn touches_planning(&self) -> bool {
        self.priority.is_some() || self.due_change.is_some() || self.plan_window_change.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupTasks {
    pub group: TaskGroupDetails,
    pub tasks: Vec<Task>,
}

impl GroupTasks {
    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|task| !task.is_done).count()
    }

    pub fn done_count(&self) -> usize {
        self.tasks.len() - self.open_count()
    }

    pub fn affected_group(&self) -> AffectedGroup {
        AffectedGroup {
            id: self.group.id,
            open_task_count: i64::try_from(self.open_count()).unwrap_or(i64::MAX),
            task_count: i64::try_from(self.tasks.len()).unwrap_or(i64::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AffectedGroup {
    pub id: Uuid,
    pub open_task_count: i64,
    pub task_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedTask {
    pub left_group_tasks: GroupTasks,
    pub affected_groups: Vec<AffectedGroup>,
}

pub fn validate_task_name(raw_name: &str) -> Result<String, TaskError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > TASK_NAME_MAX_CHARACTERS {
        return Err(TaskError::Invalid(String::from(
            "a task name is 1 to 500 characters",
        )));
    }
    Ok(trimmed_name.to_owned())
}

pub fn validate_estimate_minutes(raw_estimate_minutes: i64) -> Result<i32, TaskError> {
    if !ESTIMATE_MINUTES_RANGE.contains(&raw_estimate_minutes) {
        return Err(TaskError::Invalid(String::from(
            "an estimate is 1 minute to 24 hours",
        )));
    }
    i32::try_from(raw_estimate_minutes)
        .map_err(|_| TaskError::Invalid(String::from("an estimate is 1 minute to 24 hours")))
}

pub fn parse_task_priority(priority_text: &str) -> Result<TaskPriority, TaskError> {
    TaskPriority::parse(priority_text)
        .ok_or_else(|| TaskError::Invalid(String::from("a priority is high, normal or low")))
}

pub fn parse_task_due(date_text: &str, time_text: Option<&str>) -> Result<TaskDue, TaskError> {
    let date = parse_calendar_date(date_text)
        .ok_or_else(|| TaskError::Invalid(String::from("a date is written as YYYY-MM-DD")))?;
    let time = time_text.map(parse_task_time).transpose()?;
    Ok(TaskDue { date, time })
}

pub fn parse_plan_window(start_text: &str, end_text: &str) -> Result<WallClockWindow, TaskError> {
    WallClockWindow::new(parse_task_time(start_text)?, parse_task_time(end_text)?).ok_or_else(
        || TaskError::Invalid(String::from("a window starts and ends at different times")),
    )
}

fn parse_task_time(time_text: &str) -> Result<Time, TaskError> {
    parse_wall_clock_time(time_text)
        .ok_or_else(|| TaskError::Invalid(String::from("a time is written as HH:MM")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(is_done: bool) -> Task {
        Task {
            id: Uuid::now_v7(),
            parent_id: None,
            position: 0,
            name: String::from("A task"),
            is_done,
            estimate_minutes: None,
            card: None,
            tracked_event_count: 0,
            planning: TaskPlanning::default(),
        }
    }

    #[test]
    fn counts_open_and_done_tasks() {
        let group_tasks = GroupTasks {
            group: TaskGroupDetails {
                id: Uuid::now_v7(),
                name: String::from("Today"),
            },
            tasks: vec![task(false), task(true), task(false)],
        };

        assert_eq!(group_tasks.open_count(), 2);
        assert_eq!(group_tasks.done_count(), 1);
    }

    #[test]
    fn trims_and_limits_a_task_name() {
        assert_eq!(validate_task_name(" Plan ").expect("valid"), "Plan");
        assert!(validate_task_name("").is_err());
        assert!(validate_task_name(&"a".repeat(500)).is_ok());
        assert!(validate_task_name(&"a".repeat(501)).is_err());
    }

    #[test]
    fn high_priority_sorts_before_normal_and_low() {
        assert!(TaskPriority::High < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::Low);
        assert_eq!(TaskPriority::parse("low"), Some(TaskPriority::Low));
        assert_eq!(TaskPriority::parse("Low"), None);
    }

    #[test]
    fn reads_a_due_date_with_an_optional_time() {
        let due = parse_task_due("2026-11-21", Some("17:00")).expect("valid");

        assert_eq!(due.date, jiff::civil::date(2026, 11, 21));
        assert_eq!(due.time, Some(jiff::civil::time(17, 0, 0, 0)));
        assert!(parse_task_due("2026-02-30", None).is_err());
        assert!(parse_task_due("2026-11-21", Some("5pm")).is_err());
    }

    #[test]
    fn accepts_estimates_from_one_minute_to_a_day() {
        assert_eq!(validate_estimate_minutes(1).expect("valid"), 1);
        assert_eq!(validate_estimate_minutes(1440).expect("valid"), 1440);
        for rejected_minutes in [0, -5, 1441, i64::MAX] {
            assert!(validate_estimate_minutes(rejected_minutes).is_err());
        }
    }
}
