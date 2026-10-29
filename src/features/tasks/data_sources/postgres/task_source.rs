use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::tasks::entities::task::{NewTask, Task, TaskChanges};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskPlace {
    pub parent_id: Option<Uuid>,
    pub position: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupTaskCount {
    pub open_task_count: i64,
    pub task_count: i64,
}

#[derive(Default)]
pub struct PostgresTaskSource;

impl PostgresTaskSource {
    pub async fn list_for_group(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
    ) -> Result<Vec<Task>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id,
                      parent_id,
                      position,
                      name,
                      completed_at IS NOT NULL AS "is_done!",
                      estimate_minutes
               FROM tasks
               WHERE group_id = $1
               ORDER BY parent_id NULLS FIRST, position, id"#,
            task_group_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn find_group_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT group_id FROM tasks WHERE id = $1 AND user_id = $2",
            task_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_depth(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"WITH RECURSIVE task_ancestry AS (
                   SELECT id, parent_id, 1 AS depth
                   FROM tasks
                   WHERE id = $1 AND group_id = $2
                   UNION ALL
                   SELECT tasks.id, tasks.parent_id, task_ancestry.depth + 1
                   FROM tasks
                   JOIN task_ancestry ON tasks.id = task_ancestry.parent_id
               )
               SELECT max(depth) AS "depth" FROM task_ancestry"#,
            task_id,
            task_group_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn next_position(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        parent_task_id: Option<Uuid>,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT coalesce(max(position) + 1, 0) AS "next_position!"
               FROM tasks
               WHERE group_id = $1 AND parent_id IS NOT DISTINCT FROM $2"#,
            task_group_id,
            parent_task_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_task: &NewTask,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
             VALUES ($1, $2, $3, $4, $5, $6)",
            new_task.id,
            new_task.user_id,
            new_task.group_id,
            new_task.parent_id,
            new_task.position,
            new_task.name,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn complete_task_and_descendants(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "WITH RECURSIVE task_subtree AS (
                 SELECT id
                 FROM tasks
                 WHERE id = $1 AND group_id = $2
                 UNION ALL
                 SELECT tasks.id
                 FROM tasks
                 JOIN task_subtree ON tasks.parent_id = task_subtree.id
             )
             UPDATE tasks
             SET completed_at = now(), updated_at = now()
             WHERE id IN (SELECT id FROM task_subtree) AND completed_at IS NULL",
            task_id,
            task_group_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn insert_many(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
        new_tasks: &[NewTask],
    ) -> Result<(), sqlx::Error> {
        let task_ids: Vec<Uuid> = new_tasks.iter().map(|new_task| new_task.id).collect();
        let parent_task_ids: Vec<Option<Uuid>> = new_tasks
            .iter()
            .map(|new_task| new_task.parent_id)
            .collect();
        let positions: Vec<i32> = new_tasks.iter().map(|new_task| new_task.position).collect();
        let task_names: Vec<&str> = new_tasks
            .iter()
            .map(|new_task| new_task.name.as_str())
            .collect();
        sqlx::query!(
            r#"INSERT INTO tasks (id, user_id, group_id, parent_id, position, name)
               SELECT new_task.id, $1, $2, new_task.parent_id, new_task.position, new_task.name
               FROM UNNEST($3::uuid[], $4::uuid[], $5::integer[], $6::text[])
                    WITH ORDINALITY AS new_task (id, parent_id, position, name, insert_order)
               ORDER BY new_task.insert_order"#,
            user_id,
            task_group_id,
            &task_ids,
            &parent_task_ids as &[Option<Uuid>],
            &positions,
            &task_names as &[&str],
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn reopen_task_and_ancestors(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "WITH RECURSIVE task_ancestry AS (
                 SELECT id, parent_id
                 FROM tasks
                 WHERE id = $1 AND group_id = $2
                 UNION ALL
                 SELECT tasks.id, tasks.parent_id
                 FROM tasks
                 JOIN task_ancestry ON tasks.id = task_ancestry.parent_id
             )
             UPDATE tasks
             SET completed_at = NULL, updated_at = now()
             WHERE id IN (SELECT id FROM task_ancestry) AND completed_at IS NOT NULL",
            task_id,
            task_group_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
        task_changes: &TaskChanges,
    ) -> Result<bool, sqlx::Error> {
        let update_outcome = sqlx::query!(
            "UPDATE tasks
             SET name = coalesce($3, name),
                 estimate_minutes = CASE WHEN $4 THEN $5 ELSE estimate_minutes END,
                 updated_at = now()
             WHERE id = $1 AND group_id = $2",
            task_id,
            task_group_id,
            task_changes.name.as_deref(),
            task_changes.estimate_change.is_some(),
            task_changes.estimate_change.flatten(),
        )
        .execute(executor)
        .await?;
        Ok(update_outcome.rows_affected() == 1)
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
    ) -> Result<Option<TaskPlace>, sqlx::Error> {
        sqlx::query_as!(
            TaskPlace,
            "DELETE FROM tasks WHERE id = $1 AND group_id = $2 RETURNING parent_id, position",
            task_id,
            task_group_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn close_position_gap(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_place: TaskPlace,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks
             SET position = position - 1
             WHERE group_id = $1 AND parent_id IS NOT DISTINCT FROM $2 AND position > $3",
            task_group_id,
            task_place.parent_id,
            task_place.position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn find_place(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_id: Uuid,
    ) -> Result<Option<TaskPlace>, sqlx::Error> {
        sqlx::query_as!(
            TaskPlace,
            "SELECT parent_id, position FROM tasks WHERE id = $1 AND group_id = $2",
            task_id,
            task_group_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn subtree_contains(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        subtree_root_id: Uuid,
        candidate_task_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar!(
            r#"WITH RECURSIVE task_subtree AS (
                   SELECT id FROM tasks WHERE id = $1 AND group_id = $2
                   UNION ALL
                   SELECT tasks.id
                   FROM tasks
                   JOIN task_subtree ON tasks.parent_id = task_subtree.id
               )
               SELECT EXISTS (SELECT 1 FROM task_subtree WHERE id = $3) AS "is_contained!""#,
            subtree_root_id,
            task_group_id,
            candidate_task_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn subtree_height(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        subtree_root_id: Uuid,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar!(
            r#"WITH RECURSIVE task_subtree AS (
                   SELECT id, 1 AS level FROM tasks WHERE id = $1 AND group_id = $2
                   UNION ALL
                   SELECT tasks.id, task_subtree.level + 1
                   FROM tasks
                   JOIN task_subtree ON tasks.parent_id = task_subtree.id
               )
               SELECT coalesce(max(level), 0) AS "height!" FROM task_subtree"#,
            subtree_root_id,
            task_group_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn subtree_has_open_task(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        subtree_root_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar!(
            r#"WITH RECURSIVE task_subtree AS (
                   SELECT id, completed_at FROM tasks WHERE id = $1 AND group_id = $2
                   UNION ALL
                   SELECT tasks.id, tasks.completed_at
                   FROM tasks
                   JOIN task_subtree ON tasks.parent_id = task_subtree.id
               )
               SELECT EXISTS (SELECT 1 FROM task_subtree WHERE completed_at IS NULL) AS "has_open_task!""#,
            subtree_root_id,
            task_group_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn count_siblings_except(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        parent_task_id: Option<Uuid>,
        excluded_task_id: Uuid,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*)::integer AS "sibling_count!"
               FROM tasks
               WHERE group_id = $1 AND parent_id IS NOT DISTINCT FROM $2 AND id <> $3"#,
            task_group_id,
            parent_task_id,
            excluded_task_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn open_position_gap(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
        task_place: TaskPlace,
        excluded_task_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks
             SET position = position + 1
             WHERE group_id = $1 AND parent_id IS NOT DISTINCT FROM $2
               AND position >= $3 AND id <> $4",
            task_group_id,
            task_place.parent_id,
            task_place.position,
            excluded_task_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn place(
        &self,
        executor: impl PgExecutor<'_>,
        source_group_id: Uuid,
        task_id: Uuid,
        destination_group_id: Uuid,
        task_place: TaskPlace,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks
             SET group_id = $3, parent_id = $4, position = $5, updated_at = now()
             WHERE id = $1 AND group_id = $2",
            task_id,
            source_group_id,
            destination_group_id,
            task_place.parent_id,
            task_place.position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn count_for_group(
        &self,
        executor: impl PgExecutor<'_>,
        task_group_id: Uuid,
    ) -> Result<GroupTaskCount, sqlx::Error> {
        sqlx::query_as!(
            GroupTaskCount,
            r#"SELECT count(*) FILTER (WHERE completed_at IS NULL) AS "open_task_count!",
                      count(*) AS "task_count!"
               FROM tasks
               WHERE group_id = $1"#,
            task_group_id,
        )
        .fetch_one(executor)
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::task_groups::entities::task_group::TaskGroup;

    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
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

    pub async fn seed_task_group(db_pool: &PgPool, user_id: Uuid) -> TaskGroup {
        CreateTaskGroup::new()
            .execute(db_pool, user_id, "Today")
            .await
            .expect("the group should be created")
    }

    pub async fn seed_task(
        db_pool: &PgPool,
        user_id: Uuid,
        task_group_id: Uuid,
        parent_task_id: Option<Uuid>,
    ) -> Uuid {
        let task_source = PostgresTaskSource;
        let next_position = task_source
            .next_position(db_pool, task_group_id, parent_task_id)
            .await
            .expect("the next position should be found");
        let new_task = NewTask {
            id: Uuid::now_v7(),
            user_id,
            group_id: task_group_id,
            parent_id: parent_task_id,
            position: next_position,
            name: String::from("A task"),
        };
        task_source
            .insert(db_pool, &new_task)
            .await
            .expect("the task should be inserted");
        new_task.id
    }

    #[sqlx::test]
    async fn appends_siblings_in_order_per_parent(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let first_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        seed_task(&db_pool, owner_id, task_group.id, None).await;
        seed_task(&db_pool, owner_id, task_group.id, Some(first_task_id)).await;

        let listed_places: Vec<(Option<Uuid>, i32)> = PostgresTaskSource
            .list_for_group(&db_pool, task_group.id)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|task| (task.parent_id, task.position))
            .collect();

        assert_eq!(
            listed_places,
            vec![(None, 0), (None, 1), (Some(first_task_id), 0)]
        );
    }

    #[sqlx::test]
    async fn measures_depth_only_inside_the_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let other_group = seed_task_group(&db_pool, owner_id).await;
        let top_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let middle_task_id = seed_task(&db_pool, owner_id, task_group.id, Some(top_task_id)).await;
        let bottom_task_id =
            seed_task(&db_pool, owner_id, task_group.id, Some(middle_task_id)).await;
        let task_source = PostgresTaskSource;

        let bottom_depth = task_source
            .find_depth(&db_pool, task_group.id, bottom_task_id)
            .await
            .expect("depth should run");
        let wrong_group_depth = task_source
            .find_depth(&db_pool, other_group.id, bottom_task_id)
            .await
            .expect("depth should run");

        assert_eq!(bottom_depth, Some(3));
        assert_eq!(wrong_group_depth, None);
    }

    #[sqlx::test]
    async fn reopens_a_task_and_its_done_ancestors_only(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let top_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let middle_task_id = seed_task(&db_pool, owner_id, task_group.id, Some(top_task_id)).await;
        seed_task(&db_pool, owner_id, task_group.id, Some(middle_task_id)).await;
        let neighbour_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        for done_task_id in [top_task_id, neighbour_task_id] {
            PostgresTaskSource
                .complete_task_and_descendants(&db_pool, task_group.id, done_task_id)
                .await
                .expect("completing should run");
        }

        PostgresTaskSource
            .reopen_task_and_ancestors(&db_pool, task_group.id, middle_task_id)
            .await
            .expect("reopening should run");

        let open_task_ids: Vec<Uuid> = PostgresTaskSource
            .list_for_group(&db_pool, task_group.id)
            .await
            .expect("listing should run")
            .into_iter()
            .filter(|task| !task.is_done)
            .map(|task| task.id)
            .collect();
        assert_eq!(open_task_ids, vec![top_task_id, middle_task_id]);
    }

    #[sqlx::test]
    async fn completes_a_task_and_its_open_descendants_only(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let top_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let middle_task_id = seed_task(&db_pool, owner_id, task_group.id, Some(top_task_id)).await;
        let bottom_task_id =
            seed_task(&db_pool, owner_id, task_group.id, Some(middle_task_id)).await;
        let neighbour_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;

        PostgresTaskSource
            .complete_task_and_descendants(&db_pool, task_group.id, middle_task_id)
            .await
            .expect("completing should run");

        let done_task_ids: Vec<Uuid> = PostgresTaskSource
            .list_for_group(&db_pool, task_group.id)
            .await
            .expect("listing should run")
            .into_iter()
            .filter(|task| task.is_done)
            .map(|task| task.id)
            .collect();
        assert_eq!(done_task_ids, vec![middle_task_id, bottom_task_id]);
        assert!(!done_task_ids.contains(&top_task_id));
        assert!(!done_task_ids.contains(&neighbour_task_id));
    }

    #[sqlx::test]
    async fn deletes_and_closes_the_gap_among_siblings(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        seed_task(&db_pool, owner_id, task_group.id, None).await;
        let doomed_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        seed_task(&db_pool, owner_id, task_group.id, None).await;
        let task_source = PostgresTaskSource;

        let task_place = task_source
            .delete(&db_pool, task_group.id, doomed_task_id)
            .await
            .expect("delete should run")
            .expect("the task should exist");
        task_source
            .close_position_gap(&db_pool, task_group.id, task_place)
            .await
            .expect("closing the gap should run");

        let remaining_positions: Vec<i32> = task_source
            .list_for_group(&db_pool, task_group.id)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|task| task.position)
            .collect();
        assert_eq!(remaining_positions, vec![0, 1]);
    }

    #[sqlx::test]
    async fn updates_only_inside_the_given_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let other_group = seed_task_group(&db_pool, owner_id).await;
        let task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let rename = TaskChanges {
            name: Some(String::from("Renamed")),
            estimate_change: Some(Some(30)),
        };
        let task_source = PostgresTaskSource;

        let wrong_group_update = task_source
            .update(&db_pool, other_group.id, task_id, &rename)
            .await
            .expect("update should run");
        let right_group_update = task_source
            .update(&db_pool, task_group.id, task_id, &rename)
            .await
            .expect("update should run");

        assert!(!wrong_group_update);
        assert!(right_group_update);
    }

    #[sqlx::test]
    async fn finds_subtree_membership_height_and_open_tasks(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let top_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let middle_task_id = seed_task(&db_pool, owner_id, task_group.id, Some(top_task_id)).await;
        let bottom_task_id =
            seed_task(&db_pool, owner_id, task_group.id, Some(middle_task_id)).await;
        let neighbour_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let task_source = PostgresTaskSource;
        task_source
            .complete_task_and_descendants(&db_pool, task_group.id, middle_task_id)
            .await
            .expect("completing should run");

        assert!(
            task_source
                .subtree_contains(&db_pool, task_group.id, top_task_id, bottom_task_id)
                .await
                .expect("the check should run")
        );
        assert!(
            !task_source
                .subtree_contains(&db_pool, task_group.id, middle_task_id, neighbour_task_id)
                .await
                .expect("the check should run")
        );
        assert_eq!(
            task_source
                .subtree_height(&db_pool, task_group.id, top_task_id)
                .await
                .expect("the height should run"),
            3
        );
        assert!(
            task_source
                .subtree_has_open_task(&db_pool, task_group.id, top_task_id)
                .await
                .expect("the check should run")
        );
        assert!(
            !task_source
                .subtree_has_open_task(&db_pool, task_group.id, middle_task_id)
                .await
                .expect("the check should run")
        );
    }

    #[sqlx::test]
    async fn opens_a_gap_and_places_a_task_into_it(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let task_group = seed_task_group(&db_pool, owner_id).await;
        let first_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let second_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let moving_task_id = seed_task(&db_pool, owner_id, task_group.id, None).await;
        let task_source = PostgresTaskSource;
        let target_place = TaskPlace {
            parent_id: None,
            position: 1,
        };

        task_source
            .open_position_gap(&db_pool, task_group.id, target_place, moving_task_id)
            .await
            .expect("opening the gap should run");
        task_source
            .place(
                &db_pool,
                task_group.id,
                moving_task_id,
                task_group.id,
                target_place,
            )
            .await
            .expect("placing should run");

        let ordered_task_ids: Vec<Uuid> = {
            let mut listed_tasks = task_source
                .list_for_group(&db_pool, task_group.id)
                .await
                .expect("listing should run");
            listed_tasks.sort_by_key(|task| task.position);
            listed_tasks.into_iter().map(|task| task.id).collect()
        };
        assert_eq!(
            ordered_task_ids,
            vec![first_task_id, moving_task_id, second_task_id]
        );
        assert_eq!(
            task_source
                .count_siblings_except(&db_pool, task_group.id, None, moving_task_id)
                .await
                .expect("counting should run"),
            2
        );
        assert_eq!(
            task_source
                .count_for_group(&db_pool, task_group.id)
                .await
                .expect("counting should run"),
            GroupTaskCount {
                open_task_count: 3,
                task_count: 3,
            }
        );
    }
}
