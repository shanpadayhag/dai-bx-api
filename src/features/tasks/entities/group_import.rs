use serde_json::Value;

use crate::features::tasks::entities::task::{MAX_TASK_DEPTH, TASK_NAME_MAX_CHARACTERS};

pub const MAX_IMPORT_BYTES: usize = 1024 * 1024;
pub const MAX_IMPORTED_TASKS: usize = 2000;
const GROUP_NAME_MAX_CHARACTERS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTask {
    pub name: String,
    pub parent_index: Option<usize>,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupImport {
    pub name: String,
    pub tasks: Vec<ImportedTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportProblem {
    pub message: &'static str,
    pub path: String,
}

fn problem_at(message: &'static str, path: impl Into<String>) -> ImportProblem {
    ImportProblem {
        message,
        path: path.into(),
    }
}

fn read_group_name(group_object: &serde_json::Map<String, Value>) -> Result<String, ImportProblem> {
    let trimmed_name = group_object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|trimmed_name| !trimmed_name.is_empty())
        .ok_or_else(|| problem_at("the group needs a name", "name"))?;
    if trimmed_name.chars().count() > GROUP_NAME_MAX_CHARACTERS {
        return Err(problem_at("a group name is 1 to 200 characters", "name"));
    }
    Ok(trimmed_name.to_owned())
}

fn read_task_name(task_value: &Value, task_path: &str) -> Result<String, ImportProblem> {
    let trimmed_name = task_value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|trimmed_name| !trimmed_name.is_empty())
        .ok_or_else(|| problem_at("every task needs a name", format!("{task_path}.name")))?;
    if trimmed_name.chars().count() > TASK_NAME_MAX_CHARACTERS {
        return Err(problem_at(
            "a task name is 1 to 500 characters",
            format!("{task_path}.name"),
        ));
    }
    Ok(trimmed_name.to_owned())
}

fn read_task_list(
    tasks_value: Option<&Value>,
    tasks_path: &str,
    depth: usize,
    parent_index: Option<usize>,
    imported_tasks: &mut Vec<ImportedTask>,
) -> Result<(), ImportProblem> {
    let task_values = tasks_value
        .and_then(Value::as_array)
        .ok_or_else(|| problem_at("tasks must be a list", tasks_path))?;
    for (sibling_index, task_value) in task_values.iter().enumerate() {
        let task_path = format!("{tasks_path}[{sibling_index}]");
        if imported_tasks.len() >= MAX_IMPORTED_TASKS {
            return Err(problem_at(
                "a group file holds at most 2,000 tasks",
                task_path,
            ));
        }
        if depth > usize::try_from(MAX_TASK_DEPTH).unwrap_or(usize::MAX) {
            return Err(problem_at("tasks nest at most 32 levels deep", task_path));
        }
        if !task_value.is_object() {
            return Err(problem_at("each task is a JSON object", task_path));
        }
        let task_name = read_task_name(task_value, &task_path)?;
        let task_index = imported_tasks.len();
        imported_tasks.push(ImportedTask {
            name: task_name,
            parent_index,
            position: i32::try_from(sibling_index).unwrap_or(i32::MAX),
        });
        read_task_list(
            task_value.get("tasks"),
            &format!("{task_path}.tasks"),
            depth + 1,
            Some(task_index),
            imported_tasks,
        )?;
    }
    Ok(())
}

pub fn parse_group_import(import_bytes: &[u8]) -> Result<GroupImport, ImportProblem> {
    let import_value: Value = serde_json::from_slice(import_bytes)
        .map_err(|_| problem_at("the file isn't valid JSON", ""))?;
    let group_object = import_value
        .as_object()
        .ok_or_else(|| problem_at("a group file is a JSON object", ""))?;
    let group_name = read_group_name(group_object)?;
    let mut imported_tasks = Vec::new();
    read_task_list(
        group_object.get("tasks"),
        "tasks",
        1,
        None,
        &mut imported_tasks,
    )?;
    Ok(GroupImport {
        name: group_name,
        tasks: imported_tasks,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn parse(import_value: Value) -> Result<GroupImport, ImportProblem> {
        parse_group_import(import_value.to_string().as_bytes())
    }

    #[test]
    fn flattens_the_tree_parents_first_with_positions_per_sibling_list() {
        let group_import = parse(json!({
            "name": " Today ",
            "tasks": [
                { "name": "A", "tasks": [
                    { "name": "A1", "tasks": [] },
                    { "name": "A2", "tasks": [{ "name": "A2a", "tasks": [] }] },
                ] },
                { "name": "B", "tasks": [], "ignored": true },
            ],
        }))
        .expect("the file should parse");

        assert_eq!(group_import.name, "Today");
        let flattened: Vec<(&str, Option<usize>, i32)> = group_import
            .tasks
            .iter()
            .map(|task| (task.name.as_str(), task.parent_index, task.position))
            .collect();
        assert_eq!(
            flattened,
            vec![
                ("A", None, 0),
                ("A1", Some(0), 0),
                ("A2", Some(0), 1),
                ("A2a", Some(2), 0),
                ("B", None, 1),
            ]
        );
    }

    #[test]
    fn accepts_an_empty_task_list() {
        assert_eq!(
            parse(json!({ "name": "Empty", "tasks": [] }))
                .expect("the file should parse")
                .tasks,
            Vec::new()
        );
    }

    #[test]
    fn reports_invalid_json_at_the_root() {
        assert_eq!(
            parse_group_import(b"{ nope"),
            Err(problem_at("the file isn't valid JSON", ""))
        );
    }

    #[test]
    fn reports_the_first_problem_in_document_order() {
        assert_eq!(
            parse(json!({
                "name": "Today",
                "tasks": [
                    { "name": "Fine", "tasks": [{ "name": "", "tasks": [] }] },
                    { "tasks": [] },
                ],
            })),
            Err(problem_at(
                "every task needs a name",
                "tasks[0].tasks[0].name"
            ))
        );
    }

    #[test]
    fn stops_at_the_2001st_task() {
        let flat_tasks: Vec<Value> = (0..2001)
            .map(|task_index| json!({ "name": format!("T{task_index}"), "tasks": [] }))
            .collect();

        assert_eq!(
            parse(json!({ "name": "Big", "tasks": flat_tasks })),
            Err(problem_at(
                "a group file holds at most 2,000 tasks",
                "tasks[2000]"
            ))
        );
    }

    #[test]
    fn accepts_32_levels_and_refuses_the_33rd() {
        let chain = |chain_length: usize| {
            (0..chain_length).fold(
                json!([]),
                |child_tasks, _| json!([{ "name": "Level", "tasks": child_tasks }]),
            )
        };

        assert!(parse(json!({ "name": "Deep", "tasks": chain(32) })).is_ok());
        let too_deep_problem = parse(json!({ "name": "Deep", "tasks": chain(33) }))
            .expect_err("33 levels should be refused");
        assert_eq!(
            too_deep_problem.message,
            "tasks nest at most 32 levels deep"
        );
        assert_eq!(too_deep_problem.path.matches("tasks[0]").count(), 33);
    }
}
