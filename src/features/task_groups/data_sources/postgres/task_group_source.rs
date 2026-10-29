use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::task_groups::entities::task_group::{
    NewTaskGroup, TaskGroup, TaskGroupDetails,
};

#[derive(Default)]
pub struct PostgresTaskGroupSource;

impl PostgresTaskGroupSource {
    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<TaskGroup>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      coalesce(group_task_counts.open_task_count, 0) AS "open_task_count!",
                      coalesce(group_task_counts.task_count, 0) AS "task_count!"
               FROM task_groups
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
        .await
    }

    pub async fn find_details(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<Option<TaskGroupDetails>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroupDetails,
            "SELECT id, name FROM task_groups WHERE id = $1 AND user_id = $2",
            task_group_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn lock_details(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
    ) -> Result<Option<TaskGroupDetails>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroupDetails,
            "SELECT id, name FROM task_groups WHERE id = $1 AND user_id = $2 FOR UPDATE",
            task_group_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_task_group: &NewTaskGroup,
    ) -> Result<TaskGroup, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"INSERT INTO task_groups (id, user_id, name)
               VALUES ($1, $2, $3)
               RETURNING id, name, 0::bigint AS "open_task_count!", 0::bigint AS "task_count!""#,
            new_task_group.id,
            new_task_group.user_id,
            new_task_group.name,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn rename(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
        task_group_name: &str,
    ) -> Result<Option<TaskGroup>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"UPDATE task_groups
               SET name = $3, updated_at = now()
               WHERE id = $1 AND user_id = $2
               RETURNING id,
                         name,
                         (SELECT count(*) FROM tasks
                          WHERE tasks.group_id = task_groups.id
                            AND tasks.completed_at IS NULL) AS "open_task_count!",
                         (SELECT count(*) FROM tasks
                          WHERE tasks.group_id = task_groups.id) AS "task_count!""#,
            task_group_id,
            user_id,
            task_group_name,
        )
        .fetch_optional(executor)
        .await
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

    pub async fn seed_task_group(db_pool: &PgPool, user_id: Uuid, name: &str) -> TaskGroup {
        PostgresTaskGroupSource
            .insert(
                db_pool,
                &NewTaskGroup {
                    id: Uuid::now_v7(),
                    user_id,
                    name: name.to_owned(),
                },
            )
            .await
            .expect("insert should run")
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
    async fn renames_a_group_and_keeps_its_counts(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let shift_group = seed_task_group(&db_pool, owner_id, "Shift").await;
        seed_task(&db_pool, owner_id, shift_group.id, "Clock in", None).await;

        let renamed_group = PostgresTaskGroupSource
            .rename(&db_pool, owner_id, shift_group.id, "Night shift")
            .await
            .expect("rename should run")
            .expect("the owner's group should be found");

        assert_eq!(renamed_group.name, "Night shift");
        assert_eq!(renamed_group.open_task_count, 1);
        assert_eq!(renamed_group.task_count, 1);
    }

    #[sqlx::test]
    async fn updates_and_deletes_only_the_owner_s_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_group = seed_task_group(&db_pool, owner_id, "Mine").await;
        let foreign_update = PostgresTaskGroupSource
            .rename(&db_pool, other_person_id, owner_group.id, "Taken")
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
