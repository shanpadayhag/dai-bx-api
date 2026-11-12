use jiff::civil::Time;
use serde::Serialize;

use crate::core::recurrence::RepeatRule;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::entities::group_timetable::{
    GroupTimetable, ShowRule, UncheckRule,
};
use crate::features::task_groups::entities::task_group::ScheduledTaskGroup;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowsResponse {
    pub repeat: RepeatRule,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnchecksResponse {
    pub repeat: RepeatRule,
    pub time: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableResponse {
    pub due_now: bool,
    pub due_until: Option<String>,
    pub next_shows_at: Option<String>,
    pub last_uncheck_at: Option<String>,
    pub next_uncheck_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroupResponse {
    pub id: String,
    pub name: String,
    pub open_task_count: i64,
    pub task_count: i64,
    pub shows: Option<ShowsResponse>,
    pub unchecks: Option<UnchecksResponse>,
    pub timetable: TimetableResponse,
}

fn wall_clock_text(time: Time) -> String {
    time.strftime("%H:%M").to_string()
}

fn instant_text(instant: Option<UtcInstant>) -> Option<String> {
    instant.map(UtcInstant::to_iso_text)
}

impl From<ShowRule> for ShowsResponse {
    fn from(show_rule: ShowRule) -> Self {
        Self {
            repeat: show_rule.repeat_rule,
            window_start: show_rule
                .window
                .map(|show_window| wall_clock_text(show_window.start)),
            window_end: show_rule
                .window
                .map(|show_window| wall_clock_text(show_window.end)),
        }
    }
}

impl From<UncheckRule> for UnchecksResponse {
    fn from(uncheck_rule: UncheckRule) -> Self {
        Self {
            repeat: uncheck_rule.repeat_rule,
            time: wall_clock_text(uncheck_rule.time),
        }
    }
}

impl From<GroupTimetable> for TimetableResponse {
    fn from(group_timetable: GroupTimetable) -> Self {
        Self {
            due_now: group_timetable.is_due_now,
            due_until: instant_text(group_timetable.due_until),
            next_shows_at: instant_text(group_timetable.next_shows_at),
            last_uncheck_at: instant_text(group_timetable.last_uncheck_at),
            next_uncheck_at: instant_text(group_timetable.next_uncheck_at),
        }
    }
}

impl From<ScheduledTaskGroup> for TaskGroupResponse {
    fn from(scheduled_task_group: ScheduledTaskGroup) -> Self {
        let ScheduledTaskGroup {
            task_group,
            rules,
            timetable,
        } = scheduled_task_group;
        Self {
            id: task_group.id.to_string(),
            name: task_group.name,
            open_task_count: task_group.open_task_count,
            task_count: task_group.task_count,
            shows: rules.show_rule.map(ShowsResponse::from),
            unchecks: rules.uncheck_rule.map(UnchecksResponse::from),
            timetable: timetable.into(),
        }
    }
}
