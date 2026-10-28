use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::categories::entities::category::Category;
use crate::features::task_groups::entities::task_group::{
    NewTaskGroup, TaskGroup, TaskGroupChanges, TaskGroupDetails, TaskGroupSave,
};

#[derive(Default)]
pub struct PostgresTaskGroupSource;

struct TaskGroupRow {
    id: Uuid,
    name: String,
    category_id: Option<Uuid>,
    category_name: Option<String>,
    category_color: Option<String>,
    open_task_count: i64,
    task_count: i64,
}

impl From<TaskGroupRow> for TaskGroup {
    fn from(task_group_row: TaskGroupRow) -> Self {
        Self {
            id: task_group_row.id,
            name: task_group_row.name,
            category: category_from_columns(
                task_group_row.category_id,
                task_group_row.category_name,
                task_group_row.category_color,
            ),
            open_task_count: task_group_row.open_task_count,
            task_count: task_group_row.task_count,
        }
    }
}

struct TaskGroupDetailsRow {
    id: Uuid,
    name: String,
    category_id: Option<Uuid>,
    category_name: Option<String>,
    category_color: Option<String>,
}

fn category_from_columns(
    category_id: Option<Uuid>,
    category_name: Option<String>,
    category_color: Option<String>,
) -> Option<Category> {
    match (category_id, category_name, category_color) {
        (Some(category_id), Some(category_name), Some(category_color)) => Some(Category {
            id: category_id,
            name: category_name,
            color: category_color,
        }),
        _ => None,
    }
}

impl From<TaskGroupDetailsRow> for TaskGroupDetails {
    fn from(details_row: TaskGroupDetailsRow) -> Self {
        Self {
            id: details_row.id,
            name: details_row.name,
            category: category_from_columns(
                details_row.category_id,
                details_row.category_name,
                details_row.category_color,
            ),
        }
    }
}

fn save_outcome(
    save_result: Result<Option<TaskGroupRow>, sqlx::Error>,
) -> Result<Option<TaskGroupSave>, sqlx::Error> {
    match save_result {
        Ok(saved_row) => {
            Ok(saved_row.map(|task_group_row| TaskGroupSave::Saved(task_group_row.into())))
        }
        Err(sqlx::Error::Database(database_error)) if database_error.is_foreign_key_violation() => {
            Ok(Some(TaskGroupSave::CategoryMissing))
        }
        Err(other_error) => Err(other_error),
    }
}

impl PostgresTaskGroupSource {
    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<TaskGroup>, sqlx::Error> {
        let task_group_rows = sqlx::query_as!(
            TaskGroupRow,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      categories.id AS "category_id?",
                      categories.name::text AS "category_name?",
                      categories.color AS "category_color?",
                      coalesce(group_task_counts.open_task_count, 0) AS "open_task_count!",
                      coalesce(group_task_counts.task_count, 0) AS "task_count!"
               FROM task_groups
               LEFT JOIN categories ON categories.id = task_groups.category_id
               LEFT JOIN (
                   SELECT group_id,
                          count(*) FILTER (WHERE completed_at IS NULL) AS open_task_count,
                          count(*) AS task_count
                   FROM tasks
                   WHERE user_id = $1
                   GROUP BY group_id
               ) AS group_task_counts ON group_task_counts.group_id = task_groups.id
               WHERE task_groups.user_id = $1
               ORDER BY task_groups.created_at DESC, task_groups.id DESC"#,
            user_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(task_group_rows.into_iter().map(TaskGroup::from).collect())
    }

    pub async fn find_details(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<Option<TaskGroupDetails>, sqlx::Error> {
        let details_row = sqlx::query_as!(
            TaskGroupDetailsRow,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      categories.id AS "category_id?",
                      categories.name::text AS "category_name?",
                      categories.color AS "category_color?"
               FROM task_groups
               LEFT JOIN categories ON categories.id = task_groups.category_id
               WHERE task_groups.id = $1 AND task_groups.user_id = $2"#,
            task_group_id,
            user_id,
        )
        .fetch_optional(executor)
        .await?;
        Ok(details_row.map(TaskGroupDetails::from))
    }

    pub async fn lock_details(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<Option<TaskGroupDetails>, sqlx::Error> {
        let details_row = sqlx::query_as!(
            TaskGroupDetailsRow,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      categories.id AS "category_id?",
                      categories.name::text AS "category_name?",
                      categories.color AS "category_color?"
               FROM task_groups
               LEFT JOIN categories ON categories.id = task_groups.category_id
               WHERE task_groups.id = $1 AND task_groups.user_id = $2
               FOR UPDATE OF task_groups"#,
            task_group_id,
            user_id,
        )
        .fetch_optional(executor)
        .await?;
        Ok(details_row.map(TaskGroupDetails::from))
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_task_group: &NewTaskGroup,
    ) -> Result<TaskGroupSave, sqlx::Error> {
        let insert_result = sqlx::query_as!(
            TaskGroupRow,
            r#"WITH inserted_group AS (
                   INSERT INTO task_groups (id, user_id, name, category_id)
                   VALUES ($1, $2, $3, $4)
                   RETURNING id, name, category_id
               )
               SELECT inserted_group.id AS "id!",
                      inserted_group.name AS "name!",
                      categories.id AS "category_id?",
                      categories.name::text AS "category_name?",
                      categories.color AS "category_color?",
                      0::bigint AS "open_task_count!",
                      0::bigint AS "task_count!"
               FROM inserted_group
               LEFT JOIN categories ON categories.id = inserted_group.category_id"#,
            new_task_group.id,
            new_task_group.user_id,
            new_task_group.name,
            new_task_group.category_id,
        )
        .fetch_one(executor)
        .await
        .map(Some);
        Ok(save_outcome(insert_result)?.unwrap_or(TaskGroupSave::CategoryMissing))
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
        task_group_changes: &TaskGroupChanges,
    ) -> Result<Option<TaskGroupSave>, sqlx::Error> {
        let update_result = sqlx::query_as!(
            TaskGroupRow,
            r#"WITH updated_group AS (
                   UPDATE task_groups
                   SET name = coalesce($3, name),
                       category_id = CASE WHEN $4 THEN $5 ELSE category_id END,
                       updated_at = now()
                   WHERE id = $1 AND user_id = $2
                   RETURNING id, name, category_id
               )
               SELECT updated_group.id AS "id!",
                      updated_group.name AS "name!",
                      categories.id AS "category_id?",
                      categories.name::text AS "category_name?",
                      categories.color AS "category_color?",
                      (SELECT count(*) FROM tasks
                       WHERE tasks.group_id = updated_group.id AND tasks.completed_at IS NULL) AS "open_task_count!",
                      (SELECT count(*) FROM tasks
                       WHERE tasks.group_id = updated_group.id) AS "task_count!"
               FROM updated_group
               LEFT JOIN categories ON categories.id = updated_group.category_id"#,
            task_group_id,
            user_id,
            task_group_changes.name.as_deref(),
            task_group_changes.category_change.is_some(),
            task_group_changes.category_change.flatten(),
        )
        .fetch_optional(executor)
        .await;
        save_outcome(update_result)
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_outcome = sqlx::query!(
            "DELETE FROM task_groups WHERE id = $1 AND user_id = $2",
            task_group_id,
            user_id,
        )
        .execute(executor)
        .await?;
        Ok(delete_outcome.rows_affected() == 1)
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::categories::actions::create_category::CreateCategory;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::tasks::actions::complete_task::CompleteTask;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    pub async fn seed_user_id(db_pool: &PgPool, email: &str) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: email.to_owned(),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    pub fn new_task_group(user_id: Uuid, name: &str, category_id: Option<Uuid>) -> NewTaskGroup {
        NewTaskGroup {
            id: Uuid::now_v7(),
            user_id,
            name: name.to_owned(),
            category_id,
        }
    }

    pub async fn seed_task_group(db_pool: &PgPool, user_id: Uuid, name: &str) -> TaskGroup {
        match PostgresTaskGroupSource
            .insert(db_pool, &new_task_group(user_id, name, None))
            .await
            .expect("insert should run")
        {
            TaskGroupSave::Saved(saved_task_group) => saved_task_group,
            TaskGroupSave::CategoryMissing => panic!("a group without a category should save"),
        }
    }

    async fn seed_task(
        db_pool: &PgPool,
        user_id: Uuid,
        task_group_id: Uuid,
        task_name: &str,
        parent_task_id: Option<Uuid>,
    ) -> Uuid {
        AddTask::new(db_pool)
            .execute(user_id, task_group_id, task_name, parent_task_id)
            .await
            .expect("the task should be added")
            .tasks
            .into_iter()
            .find(|task| task.name == task_name)
            .expect("the added task should be listed")
            .id
    }

    async fn mark_done(db_pool: &PgPool, user_id: Uuid, task_ids: &[Uuid]) {
        for task_id in task_ids {
            CompleteTask::new(db_pool)
                .execute(user_id, *task_id)
                .await
                .expect("the task should be completed");
        }
    }

    #[sqlx::test]
    async fn counts_open_and_all_tasks_at_every_depth(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let busy_group = seed_task_group(&db_pool, owner_id, "Busy").await;
        let empty_group = seed_task_group(&db_pool, owner_id, "Empty").await;
        let parent_task_id = seed_task(&db_pool, owner_id, busy_group.id, "Parent", None).await;
        let child_task_id = seed_task(
            &db_pool,
            owner_id,
            busy_group.id,
            "Child",
            Some(parent_task_id),
        )
        .await;
        let grandchild_task_id = seed_task(
            &db_pool,
            owner_id,
            busy_group.id,
            "Grandchild",
            Some(child_task_id),
        )
        .await;
        let sibling_task_id = seed_task(&db_pool, owner_id, busy_group.id, "Sibling", None).await;
        mark_done(&db_pool, owner_id, &[grandchild_task_id, sibling_task_id]).await;

        let listed_counts: Vec<(String, i64, i64)> = PostgresTaskGroupSource
            .list_for_user(&db_pool, owner_id)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|task_group| {
                (
                    task_group.name,
                    task_group.open_task_count,
                    task_group.task_count,
                )
            })
            .collect();

        assert_eq!(
            listed_counts,
            vec![(String::from("Empty"), 0, 0), (String::from("Busy"), 2, 4),]
        );
        assert_eq!(empty_group.task_count, 0);
    }

    #[sqlx::test]
    async fn reports_a_category_that_is_not_the_owner_s(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category = CreateCategory::new()
            .execute(&db_pool, other_person_id, "Theirs", "#123456")
            .await
            .expect("the category should be created");
        let owner_group = seed_task_group(&db_pool, owner_id, "Mine").await;

        let foreign_insert = PostgresTaskGroupSource
            .insert(
                &db_pool,
                &new_task_group(owner_id, "Mine too", Some(other_category.id)),
            )
            .await
            .expect("insert should run");
        let foreign_update = PostgresTaskGroupSource
            .update(
                &db_pool,
                owner_id,
                owner_group.id,
                &TaskGroupChanges {
                    name: None,
                    category_change: Some(Some(other_category.id)),
                },
            )
            .await
            .expect("update should run");

        assert_eq!(foreign_insert, TaskGroupSave::CategoryMissing);
        assert_eq!(foreign_update, Some(TaskGroupSave::CategoryMissing));
    }

    #[sqlx::test]
    async fn sets_keeps_and_clears_the_category(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let work_category = CreateCategory::new()
            .execute(&db_pool, owner_id, "Work", "#c6afa3")
            .await
            .expect("the category should be created");
        let shift_group = seed_task_group(&db_pool, owner_id, "Shift").await;
        let update_group = |task_group_changes: TaskGroupChanges| {
            let db_pool = db_pool.clone();
            async move {
                match PostgresTaskGroupSource
                    .update(&db_pool, owner_id, shift_group.id, &task_group_changes)
                    .await
                    .expect("update should run")
                {
                    Some(TaskGroupSave::Saved(saved_task_group)) => saved_task_group,
                    other_outcome => panic!("unexpected outcome: {other_outcome:?}"),
                }
            }
        };

        let categorized_group = update_group(TaskGroupChanges {
            name: None,
            category_change: Some(Some(work_category.id)),
        })
        .await;
        let renamed_group = update_group(TaskGroupChanges {
            name: Some(String::from("Night shift")),
            category_change: None,
        })
        .await;
        let cleared_group = update_group(TaskGroupChanges {
            name: None,
            category_change: Some(None),
        })
        .await;

        assert_eq!(categorized_group.category, Some(work_category));
        assert!(renamed_group.category.is_some());
        assert_eq!(renamed_group.name, "Night shift");
        assert_eq!(cleared_group.category, None);
    }

    #[sqlx::test]
    async fn updates_and_deletes_only_the_owner_s_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_group = seed_task_group(&db_pool, owner_id, "Mine").await;
        let rename = TaskGroupChanges {
            name: Some(String::from("Taken")),
            category_change: None,
        };

        let foreign_update = PostgresTaskGroupSource
            .update(&db_pool, other_person_id, owner_group.id, &rename)
            .await
            .expect("update should run");
        let foreign_delete = PostgresTaskGroupSource
            .delete(&db_pool, other_person_id, owner_group.id)
            .await
            .expect("delete should run");
        let owner_delete = PostgresTaskGroupSource
            .delete(&db_pool, owner_id, owner_group.id)
            .await
            .expect("delete should run");

        assert_eq!(foreign_update, None);
        assert!(!foreign_delete);
        assert!(owner_delete);
    }
}
