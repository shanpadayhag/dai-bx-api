use uuid::Uuid;

use crate::features::task_groups::entities::task_group::TaskGroupDetails;
use crate::features::tasks::error::TaskError;

pub const TASK_NAME_MAX_CHARACTERS: usize = 500;
pub const MAX_TASK_DEPTH: i32 = 32;
pub const ESTIMATE_MINUTES_RANGE: std::ops::RangeInclusive<i64> = 1..=1440;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub name: String,
    pub is_done: bool,
    pub estimate_minutes: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub id: Uuid,
    pub user_id: Uuid,
    pub group_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskChanges {
    pub name: Option<String>,
    pub estimate_change: Option<Option<i32>>,
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
        }
    }

    #[test]
    fn counts_open_and_done_tasks() {
        let group_tasks = GroupTasks {
            group: TaskGroupDetails {
                id: Uuid::now_v7(),
                name: String::from("Today"),
                category: None,
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
    fn accepts_estimates_from_one_minute_to_a_day() {
        assert_eq!(validate_estimate_minutes(1).expect("valid"), 1);
        assert_eq!(validate_estimate_minutes(1440).expect("valid"), 1440);
        for rejected_minutes in [0, -5, 1441, i64::MAX] {
            assert!(validate_estimate_minutes(rejected_minutes).is_err());
        }
    }
}
