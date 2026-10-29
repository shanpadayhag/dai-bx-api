use uuid::Uuid;

use crate::features::task_groups::error::TaskGroupError;

pub const TASK_GROUP_NAME_MAX_CHARACTERS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGroup {
    pub id: Uuid,
    pub name: String,
    pub open_task_count: i64,
    pub task_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGroupDetails {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTaskGroup {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
}

pub fn validate_task_group_name(raw_name: &str) -> Result<String, TaskGroupError> {
    let trimmed_name = raw_name.trim();
    let name_character_count = trimmed_name.chars().count();
    if name_character_count == 0 || name_character_count > TASK_GROUP_NAME_MAX_CHARACTERS {
        return Err(TaskGroupError::Invalid(String::from(
            "a group name is 1 to 200 characters",
        )));
    }
    Ok(trimmed_name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_a_group_name() {
        assert_eq!(
            validate_task_group_name("  Today ").expect("a valid name"),
            "Today"
        );
    }

    #[test]
    fn refuses_an_empty_or_over_long_group_name() {
        assert!(validate_task_group_name(" ").is_err());
        assert!(validate_task_group_name(&"a".repeat(201)).is_err());
        assert!(validate_task_group_name(&"a".repeat(200)).is_ok());
    }
}
