use jiff::civil::{Date, Time};
use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::RepeatRule;
use crate::features::task_groups::entities::group_rules::{GroupRules, GroupRulesListing};
use crate::features::task_groups::entities::group_timetable::{ShowRule, ShowWindow, UncheckRule};
use crate::features::task_groups::entities::task_group::{
    NewTaskGroup, TaskGroup, TaskGroupDetails,
};
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Default)]
pub struct PostgresTaskGroupSource;

struct GroupRulesRow {
    id: Option<Uuid>,
    show_rule: Option<Json<RepeatRule>>,
    show_start_date_text: Option<String>,
    show_window_start_text: Option<String>,
    show_window_end_text: Option<String>,
    uncheck_rule: Option<Json<RepeatRule>>,
    uncheck_start_date_text: Option<String>,
    uncheck_time_text: Option<String>,
    calendar_time_zone: Option<String>,
}

fn decode_text<T: std::str::FromStr>(stored_text: Option<&str>) -> Result<Option<T>, sqlx::Error>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    stored_text
        .map(|stored_text| {
            stored_text
                .parse::<T>()
                .map_err(|parse_error| sqlx::Error::Decode(Box::new(parse_error)))
        })
        .transpose()
}

impl GroupRulesRow {
    fn into_group_rules(self, task_group_id: Uuid) -> Result<(Uuid, GroupRules), sqlx::Error> {
        let show_window = match (
            decode_text::<Time>(self.show_window_start_text.as_deref())?,
            decode_text::<Time>(self.show_window_end_text.as_deref())?,
        ) {
            (Some(start), Some(end)) => Some(ShowWindow { start, end }),
            _ => None,
        };
        let show_rule = match (
            self.show_rule,
            decode_text::<Date>(self.show_start_date_text.as_deref())?,
        ) {
            (Some(Json(repeat_rule)), Some(start_date)) => Some(ShowRule {
                repeat_rule,
                start_date,
                window: show_window,
            }),
            _ => None,
        };
        let uncheck_rule = match (
            self.uncheck_rule,
            decode_text::<Date>(self.uncheck_start_date_text.as_deref())?,
            decode_text::<Time>(self.uncheck_time_text.as_deref())?,
        ) {
            (Some(Json(repeat_rule)), Some(start_date), Some(time)) => Some(UncheckRule {
                repeat_rule,
                start_date,
                time,
            }),
            _ => None,
        };
        Ok((
            task_group_id,
            GroupRules {
                show_rule,
                uncheck_rule,
            },
        ))
    }
}

struct GroupRuleColumns {
    show_rule: Option<Json<RepeatRule>>,
    show_start_date_text: Option<String>,
    show_window_start_text: Option<String>,
    show_window_end_text: Option<String>,
    uncheck_rule: Option<Json<RepeatRule>>,
    uncheck_start_date_text: Option<String>,
    uncheck_time_text: Option<String>,
}

impl From<&GroupRules> for GroupRuleColumns {
    fn from(group_rules: &GroupRules) -> Self {
        let show_window = group_rules
            .show_rule
            .as_ref()
            .and_then(|show_rule| show_rule.window);
        Self {
            show_rule: group_rules
                .show_rule
                .as_ref()
                .map(|show_rule| Json(show_rule.repeat_rule.clone())),
            show_start_date_text: group_rules
                .show_rule
                .as_ref()
                .map(|show_rule| show_rule.start_date.to_string()),
            show_window_start_text: show_window.map(|show_window| show_window.start.to_string()),
            show_window_end_text: show_window.map(|show_window| show_window.end.to_string()),
            uncheck_rule: group_rules
                .uncheck_rule
                .as_ref()
                .map(|uncheck_rule| Json(uncheck_rule.repeat_rule.clone())),
            uncheck_start_date_text: group_rules
                .uncheck_rule
                .as_ref()
                .map(|uncheck_rule| uncheck_rule.start_date.to_string()),
            uncheck_time_text: group_rules
                .uncheck_rule
                .as_ref()
                .map(|uncheck_rule| uncheck_rule.time.to_string()),
        }
    }
}

impl PostgresTaskGroupSource {
    pub async fn list_rules(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        only_task_group_id: Option<Uuid>,
    ) -> Result<GroupRulesListing, sqlx::Error> {
        let rules_rows = sqlx::query_as!(
            GroupRulesRow,
            r#"SELECT task_groups.id AS "id?",
                      task_groups.show_rule AS "show_rule: Json<RepeatRule>",
                      task_groups.show_start_date::text AS show_start_date_text,
                      to_char(task_groups.show_window_start, 'HH24:MI') AS show_window_start_text,
                      to_char(task_groups.show_window_end, 'HH24:MI') AS show_window_end_text,
                      task_groups.uncheck_rule AS "uncheck_rule: Json<RepeatRule>",
                      task_groups.uncheck_start_date::text AS uncheck_start_date_text,
                      to_char(task_groups.uncheck_time, 'HH24:MI') AS uncheck_time_text,
                      users.calendar_time_zone
               FROM users
               LEFT JOIN task_groups
                    ON task_groups.user_id = users.id
                   AND ($2::uuid IS NULL OR task_groups.id = $2)
               WHERE users.id = $1"#,
            user_id,
            only_task_group_id,
        )
        .fetch_all(executor)
        .await?;
        let calendar_time_zone_name = rules_rows
            .first()
            .and_then(|rules_row| rules_row.calendar_time_zone.clone());
        let group_rules = rules_rows
            .into_iter()
            .filter_map(|rules_row| {
                rules_row
                    .id
                    .map(|task_group_id| rules_row.into_group_rules(task_group_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GroupRulesListing {
            calendar_time_zone_name,
            group_rules,
        })
    }

    pub async fn find_calendar_time_zone_name(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        Ok(sqlx::query_scalar!(
            "SELECT calendar_time_zone FROM users WHERE id = $1",
            user_id
        )
        .fetch_optional(executor)
        .await?
        .flatten())
    }

    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        only_task_group_id: Option<Uuid>,
        uncheck_cutoffs: &UncheckCutoffs,
    ) -> Result<Vec<TaskGroup>, sqlx::Error> {
        sqlx::query_as!(
            TaskGroup,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      coalesce(group_task_counts.open_task_count, 0) AS "open_task_count!",
                      coalesce(group_task_counts.task_count, 0) AS "task_count!"
               FROM task_groups
               LEFT JOIN (
                   SELECT tasks.group_id,
                          count(*) FILTER (
                              WHERE tasks.completed_at IS NULL
                                 OR tasks.completed_at
                                    < coalesce(to_timestamp(uncheck_cutoffs.cutoff_millis / 1000.0),
                                               '-infinity')
                          ) AS open_task_count,
                          count(*) AS task_count
                   FROM tasks
                   LEFT JOIN unnest($3::uuid[], $4::bigint[])
                        AS uncheck_cutoffs(group_id, cutoff_millis)
                        ON uncheck_cutoffs.group_id = tasks.group_id
                   WHERE tasks.user_id = $1
                   GROUP BY tasks.group_id
               ) AS group_task_counts ON group_task_counts.group_id = task_groups.id
               WHERE task_groups.user_id = $1
                 AND ($2::uuid IS NULL OR task_groups.id = $2)
               ORDER BY task_groups.created_at DESC, task_groups.id DESC"#,
            user_id,
            only_task_group_id,
            &uncheck_cutoffs.group_ids,
            &uncheck_cutoffs.cutoff_millis,
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
        let rule_columns = GroupRuleColumns::from(&new_task_group.rules);
        sqlx::query_as!(
            TaskGroup,
            r#"INSERT INTO task_groups
                   (id, user_id, name, show_rule, show_start_date, show_window_start,
                    show_window_end, uncheck_rule, uncheck_start_date, uncheck_time)
               VALUES ($1, $2, $3, $4, $5::text::date, $6::text::time, $7::text::time, $8,
                       $9::text::date, $10::text::time)
               RETURNING id, name, 0::bigint AS "open_task_count!", 0::bigint AS "task_count!""#,
            new_task_group.id,
            new_task_group.user_id,
            new_task_group.name,
            rule_columns.show_rule as _,
            rule_columns.show_start_date_text,
            rule_columns.show_window_start_text,
            rule_columns.show_window_end_text,
            rule_columns.uncheck_rule as _,
            rule_columns.uncheck_start_date_text,
            rule_columns.uncheck_time_text,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        task_group_id: Uuid,
        task_group_name: &str,
        group_rules: &GroupRules,
    ) -> Result<bool, sqlx::Error> {
        let rule_columns = GroupRuleColumns::from(group_rules);
        let update_outcome = sqlx::query!(
            "UPDATE task_groups
             SET name = $3,
                 show_rule = $4,
                 show_start_date = $5::text::date,
                 show_window_start = $6::text::time,
                 show_window_end = $7::text::time,
                 uncheck_rule = $8,
                 uncheck_start_date = $9::text::date,
                 uncheck_time = $10::text::time,
                 updated_at = now()
             WHERE id = $1 AND user_id = $2",
            task_group_id,
            user_id,
            task_group_name,
            rule_columns.show_rule as _,
            rule_columns.show_start_date_text,
            rule_columns.show_window_start_text,
            rule_columns.show_window_end_text,
            rule_columns.uncheck_rule as _,
            rule_columns.uncheck_start_date_text,
            rule_columns.uncheck_time_text,
        )
        .execute(executor)
        .await?;
        Ok(update_outcome.rows_affected() == 1)
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
    use crate::core::request_time_zone::RequestTimeZone;
    use sqlx::PgPool;

    use super::*;
    use crate::core::utc_instant::UtcInstant;
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
                    rules: GroupRules::default(),
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
            .execute(
                user_id,
                task_group_id,
                task_name,
                parent_task_id,
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
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
                .execute(
                    user_id,
                    *task_id,
                    &RequestTimeZone::default(),
                    UtcInstant::now(),
                )
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
            .list_for_user(&db_pool, owner_id, None, &UncheckCutoffs::default())
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

    fn weekend_rules() -> GroupRules {
        let weekends: RepeatRule = serde_json::from_str(
            r#"{"frequency":"weekly","interval":1,"weekdays":["sat","sun"],"ends":{"never":true}}"#,
        )
        .expect("the rule should parse");
        GroupRules {
            show_rule: Some(ShowRule {
                repeat_rule: weekends.clone(),
                start_date: jiff::civil::date(2026, 11, 12),
                window: Some(ShowWindow {
                    start: jiff::civil::time(22, 0, 0, 0),
                    end: jiff::civil::time(2, 0, 0, 0),
                }),
            }),
            uncheck_rule: Some(UncheckRule {
                repeat_rule: weekends,
                start_date: jiff::civil::date(2026, 11, 12),
                time: jiff::civil::time(4, 0, 0, 0),
            }),
        }
    }

    #[sqlx::test]
    async fn stores_and_reads_back_the_rules_with_the_calendar_zone(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let weekend_group = seed_task_group(&db_pool, owner_id, "Weekend").await;
        sqlx::query!(
            "UPDATE users SET calendar_time_zone = 'Asia/Manila' WHERE id = $1",
            owner_id
        )
        .execute(&db_pool)
        .await
        .expect("setting the zone should run");

        let was_updated = PostgresTaskGroupSource
            .update(
                &db_pool,
                owner_id,
                weekend_group.id,
                "Weekend",
                &weekend_rules(),
            )
            .await
            .expect("update should run");
        let rules_listing = PostgresTaskGroupSource
            .list_rules(&db_pool, owner_id, Some(weekend_group.id))
            .await
            .expect("listing rules should run");

        assert!(was_updated);
        assert_eq!(
            rules_listing,
            GroupRulesListing {
                calendar_time_zone_name: Some(String::from("Asia/Manila")),
                group_rules: vec![(weekend_group.id, weekend_rules())],
            }
        );
    }

    #[sqlx::test]
    async fn counts_ticks_before_the_cutoff_as_open(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let weekend_group = seed_task_group(&db_pool, owner_id, "Weekend").await;
        let other_group = seed_task_group(&db_pool, owner_id, "Other").await;
        let weekend_task_id = seed_task(&db_pool, owner_id, weekend_group.id, "Sweep", None).await;
        let other_task_id = seed_task(&db_pool, owner_id, other_group.id, "Read", None).await;
        mark_done(&db_pool, owner_id, &[weekend_task_id, other_task_id]).await;
        let later_cutoff = UncheckCutoffs {
            group_ids: vec![weekend_group.id],
            cutoff_millis: vec![UtcInstant::now().unix_millis() + 60_000],
        };

        let listed_counts: Vec<(String, i64)> = PostgresTaskGroupSource
            .list_for_user(&db_pool, owner_id, None, &later_cutoff)
            .await
            .expect("listing should run")
            .into_iter()
            .map(|task_group| (task_group.name, task_group.open_task_count))
            .collect();

        assert_eq!(
            listed_counts,
            vec![(String::from("Other"), 0), (String::from("Weekend"), 1)]
        );
    }

    #[sqlx::test]
    async fn updates_and_deletes_only_the_owner_s_group(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_group = seed_task_group(&db_pool, owner_id, "Mine").await;
        let foreign_update = PostgresTaskGroupSource
            .update(
                &db_pool,
                other_person_id,
                owner_group.id,
                "Taken",
                &GroupRules::default(),
            )
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

        assert!(!foreign_update);
        assert!(!foreign_delete);
        assert!(owner_delete);
    }
}
