use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::planning::entities::planning_task::{BlockTask, PlanningTask};
use crate::features::tasks::entities::task::{
    TaskDue, TaskPlanning, parse_plan_window, parse_task_due, parse_task_priority,
};

#[derive(Default)]
pub struct PostgresPlanningSource;

struct PlanningTaskRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    group_id: Uuid,
    group_name: String,
    group_created_at_millis: i64,
    position: i32,
    name: String,
    estimate_minutes: Option<i32>,
    completed_at_millis: Option<i64>,
    priority: String,
    due_date_text: Option<String>,
    due_time_text: Option<String>,
    plan_window_start_text: Option<String>,
    plan_window_end_text: Option<String>,
    card_due_date_text: Option<String>,
    card_due_time_text: Option<String>,
    has_planned_block: bool,
    is_running: bool,
}

fn stored_value_error(stored_error: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Decode(stored_error.to_string().into())
}

fn decode_due(
    date_text: Option<String>,
    time_text: Option<String>,
) -> Result<Option<TaskDue>, sqlx::Error> {
    date_text
        .map(|date_text| parse_task_due(&date_text, time_text.as_deref()))
        .transpose()
        .map_err(stored_value_error)
}

impl TryFrom<PlanningTaskRow> for PlanningTask {
    type Error = sqlx::Error;

    fn try_from(task_row: PlanningTaskRow) -> Result<Self, Self::Error> {
        let plan_window = task_row
            .plan_window_start_text
            .zip(task_row.plan_window_end_text)
            .map(|(start_text, end_text)| parse_plan_window(&start_text, &end_text))
            .transpose()
            .map_err(stored_value_error)?;
        Ok(Self {
            id: task_row.id,
            parent_id: task_row.parent_id,
            group_id: task_row.group_id,
            group_name: task_row.group_name,
            group_created_at: UtcInstant::from_unix_millis(task_row.group_created_at_millis),
            position: task_row.position,
            name: task_row.name,
            estimate_minutes: task_row.estimate_minutes,
            completed_at: task_row
                .completed_at_millis
                .map(UtcInstant::from_unix_millis),
            planning: TaskPlanning {
                priority: parse_task_priority(&task_row.priority).map_err(stored_value_error)?,
                due: decode_due(task_row.due_date_text, task_row.due_time_text)?,
                plan_window,
            },
            card_due: decode_due(task_row.card_due_date_text, task_row.card_due_time_text)?,
            has_planned_block: task_row.has_planned_block,
            is_running: task_row.is_running,
        })
    }
}

impl PostgresPlanningSource {
    pub async fn list_planning_tasks(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        group_ids: Option<&[Uuid]>,
        now: UtcInstant,
    ) -> Result<Vec<PlanningTask>, sqlx::Error> {
        let task_rows = sqlx::query_as!(
            PlanningTaskRow,
            r#"SELECT tasks.id,
                      tasks.parent_id,
                      tasks.group_id,
                      task_groups.name AS group_name,
                      (extract(epoch FROM task_groups.created_at) * 1000)::bigint AS "group_created_at_millis!",
                      tasks.position,
                      tasks.name,
                      tasks.estimate_minutes,
                      (extract(epoch FROM tasks.completed_at) * 1000)::bigint AS completed_at_millis,
                      tasks.priority,
                      tasks.due_date::text AS due_date_text,
                      to_char(tasks.due_time, 'HH24:MI') AS due_time_text,
                      to_char(tasks.plan_window_start, 'HH24:MI') AS plan_window_start_text,
                      to_char(tasks.plan_window_end, 'HH24:MI') AS plan_window_end_text,
                      cards.due_date::text AS "card_due_date_text?",
                      to_char(cards.due_time, 'HH24:MI') AS "card_due_time_text?",
                      EXISTS (
                          SELECT 1 FROM calendar_events
                          WHERE calendar_events.kind = 'planned'
                            AND calendar_events.task_id = tasks.id
                            AND calendar_events.ends_at
                                > 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
                      ) AS "has_planned_block!",
                      EXISTS (
                          SELECT 1 FROM calendar_events
                          WHERE calendar_events.kind = 'tracked'
                            AND calendar_events.task_id = tasks.id
                            AND calendar_events.ends_at IS NULL
                      ) AS "is_running!"
               FROM tasks
               JOIN task_groups ON task_groups.id = tasks.group_id
               LEFT JOIN cards ON cards.id = tasks.card_id
               WHERE tasks.user_id = $1
                 AND ($2::uuid[] IS NULL OR tasks.group_id = ANY($2))
               ORDER BY task_groups.created_at, tasks.group_id, tasks.position, tasks.id"#,
            user_id,
            group_ids,
            now.unix_millis(),
        )
        .fetch_all(executor)
        .await?;
        task_rows.into_iter().map(PlanningTask::try_from).collect()
    }

    pub async fn list_block_tasks(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_ids: &[Uuid],
    ) -> Result<Vec<BlockTask>, sqlx::Error> {
        sqlx::query_as!(
            BlockTask,
            r#"SELECT id, name, parent_id IS NULL AS "is_top_level!"
               FROM tasks
               WHERE user_id = $1 AND id = ANY($2)"#,
            user_id,
            task_ids,
        )
        .fetch_all(executor)
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::tasks::entities::task::TaskPriority;
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

    async fn seed_group_with_task(
        db_pool: &PgPool,
        user_id: Uuid,
        group_name: &str,
    ) -> (Uuid, Uuid) {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, group_name)
            .await
            .expect("the group should be created");
        let group_tasks = AddTask::new(db_pool)
            .execute(
                user_id,
                task_group.id,
                "Write the plan",
                None,
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the task should be added");
        (task_group.id, group_tasks.tasks[0].id)
    }

    #[sqlx::test]
    async fn lists_the_chosen_groups_tasks_with_their_planning_facts(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (home_group_id, home_task_id) = seed_group_with_task(&db_pool, user_id, "Home").await;
        let (work_group_id, _) = seed_group_with_task(&db_pool, user_id, "Work").await;
        seed_group_with_task(&db_pool, other_user_id, "Theirs").await;
        sqlx::query(
            "UPDATE tasks SET priority = 'high', estimate_minutes = 30,
                 due_date = '2026-11-21', due_time = '17:00',
                 plan_window_start = '22:00', plan_window_end = '02:00'
             WHERE id = $1",
        )
        .bind(home_task_id)
        .execute(&db_pool)
        .await
        .expect("the task should be updated");
        sqlx::query(
            "INSERT INTO calendar_events (id, user_id, kind, title, task_id, starts_at, ends_at)
             VALUES ($1, $2, 'planned', 'Block', $3, now() + interval '1 hour', now() + interval '2 hours'),
                    ($4, $2, 'tracked', 'Running', $3, now() - interval '1 hour', NULL)",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(home_task_id)
        .bind(Uuid::now_v7())
        .execute(&db_pool)
        .await
        .expect("the events should be inserted");

        let every_task = PostgresPlanningSource
            .list_planning_tasks(&db_pool, user_id, None, UtcInstant::now())
            .await
            .expect("the list should run");
        let home_tasks = PostgresPlanningSource
            .list_planning_tasks(&db_pool, user_id, Some(&[home_group_id]), UtcInstant::now())
            .await
            .expect("the list should run");

        let group_ids: Vec<Uuid> = every_task.iter().map(|task| task.group_id).collect();
        assert_eq!(group_ids, vec![home_group_id, work_group_id]);
        assert_eq!(home_tasks.len(), 1);
        let home_task = &home_tasks[0];
        assert_eq!(home_task.group_name, "Home");
        assert_eq!(home_task.estimate_minutes, Some(30));
        assert_eq!(home_task.planning.priority, TaskPriority::High);
        assert_eq!(
            home_task.planning.due.map(|due| due.date.to_string()),
            Some(String::from("2026-11-21"))
        );
        assert!(home_task.planning.plan_window.is_some());
        assert!(home_task.has_planned_block);
        assert!(home_task.is_running);
    }

    #[sqlx::test]
    async fn lists_only_the_owner_s_block_tasks(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (_, task_id) = seed_group_with_task(&db_pool, user_id, "Home").await;
        let (_, other_task_id) = seed_group_with_task(&db_pool, other_user_id, "Theirs").await;

        let block_tasks = PostgresPlanningSource
            .list_block_tasks(&db_pool, user_id, &[task_id, other_task_id])
            .await
            .expect("the list should run");

        assert_eq!(
            block_tasks,
            vec![BlockTask {
                id: task_id,
                name: String::from("Write the plan"),
                is_top_level: true,
            }]
        );
    }
}
