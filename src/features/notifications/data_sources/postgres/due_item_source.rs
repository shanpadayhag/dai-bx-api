use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::{WallClockWindow, parse_wall_clock_time};
use crate::features::notifications::entities::check_in_due::{CheckInSettings, RunningCheckIn};
use crate::features::notifications::entities::due_item::{
    DueScheduleRun, NotificationRecipient, NotificationWindow,
};
use crate::features::notifications::entities::notification_settings::NotificationSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningCheckInItem {
    pub user_id: Uuid,
    pub event_id: Uuid,
    pub item_title: String,
    pub context_name: Option<String>,
    pub running_check_in: RunningCheckIn,
}

struct RecipientRow {
    id: Uuid,
    notify_check_ins: bool,
    notify_schedule_runs: bool,
    notify_blocks_starting: bool,
    notify_quiet_outside_working_hours: bool,
    check_ins_enabled: bool,
    check_in_interval_minutes: i32,
    first_check_in_minutes: i32,
    working_hours_start_text: String,
    working_hours_end_text: String,
    calendar_time_zone: Option<String>,
}

impl From<RecipientRow> for NotificationRecipient {
    fn from(recipient_row: RecipientRow) -> Self {
        let working_hours = parse_wall_clock_time(&recipient_row.working_hours_start_text)
            .zip(parse_wall_clock_time(&recipient_row.working_hours_end_text))
            .and_then(|(start, end)| WallClockWindow::new(start, end));
        Self {
            user_id: recipient_row.id,
            notification_settings: NotificationSettings {
                check_ins: recipient_row.notify_check_ins,
                schedule_runs: recipient_row.notify_schedule_runs,
                blocks_starting: recipient_row.notify_blocks_starting,
                quiet_outside_working_hours: recipient_row.notify_quiet_outside_working_hours,
            },
            check_in_settings: CheckInSettings {
                enabled: recipient_row.check_ins_enabled,
                interval_minutes: i64::from(recipient_row.check_in_interval_minutes),
                first_check_in_minutes: i64::from(recipient_row.first_check_in_minutes),
            },
            working_hours,
            calendar_time_zone: recipient_row.calendar_time_zone,
        }
    }
}

struct RunningCheckInRow {
    user_id: Uuid,
    id: Uuid,
    title: String,
    context_name: Option<String>,
    estimate_minutes: Option<i32>,
    starts_at_millis: i64,
    last_answered_at_millis: Option<i64>,
}

impl From<RunningCheckInRow> for RunningCheckInItem {
    fn from(running_row: RunningCheckInRow) -> Self {
        Self {
            user_id: running_row.user_id,
            event_id: running_row.id,
            item_title: running_row.title,
            context_name: running_row.context_name,
            running_check_in: RunningCheckIn {
                started_at: UtcInstant::from_unix_millis(running_row.starts_at_millis),
                last_answered_at: running_row
                    .last_answered_at_millis
                    .map(UtcInstant::from_unix_millis),
                estimate_minutes: running_row.estimate_minutes.map(i64::from),
            },
        }
    }
}

struct ScheduleRunRow {
    user_id: Uuid,
    id: Uuid,
    card_id: Uuid,
    board_id: Uuid,
    card_title: String,
    board_name: String,
    moved_to_list_name: Option<String>,
    ran_at_millis: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PostgresDueItemSource;

impl PostgresDueItemSource {
    pub async fn list_recipients(
        &self,
        executor: impl PgExecutor<'_>,
    ) -> Result<Vec<NotificationRecipient>, sqlx::Error> {
        Ok(sqlx::query_as!(
            RecipientRow,
            r#"SELECT users.id,
                      users.notify_check_ins,
                      users.notify_schedule_runs,
                      users.notify_blocks_starting,
                      users.notify_quiet_outside_working_hours,
                      users.check_ins_enabled,
                      users.check_in_interval_minutes,
                      users.first_check_in_minutes,
                      to_char(users.working_hours_start, 'HH24:MI') AS "working_hours_start_text!",
                      to_char(users.working_hours_end, 'HH24:MI') AS "working_hours_end_text!",
                      users.calendar_time_zone
               FROM users
               WHERE EXISTS (SELECT 1 FROM push_devices WHERE push_devices.user_id = users.id)
               ORDER BY users.id"#
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(NotificationRecipient::from)
        .collect())
    }

    pub async fn list_running_check_ins(
        &self,
        executor: impl PgExecutor<'_>,
        user_ids: &[Uuid],
    ) -> Result<Vec<RunningCheckInItem>, sqlx::Error> {
        Ok(sqlx::query_as!(
            RunningCheckInRow,
            r#"SELECT calendar_events.user_id,
                      calendar_events.id,
                      calendar_events.title,
                      coalesce(task_groups.name, boards.name) AS context_name,
                      tasks.estimate_minutes AS "estimate_minutes?",
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis
               FROM calendar_events
               LEFT JOIN tasks ON tasks.id = calendar_events.task_id
               LEFT JOIN task_groups ON task_groups.id = tasks.group_id
               LEFT JOIN cards ON cards.id = calendar_events.card_id
               LEFT JOIN boards ON boards.id = cards.board_id
               WHERE calendar_events.user_id = ANY($1)
                 AND calendar_events.kind = 'tracked'
                 AND calendar_events.ends_at IS NULL
               ORDER BY calendar_events.user_id, calendar_events.starts_at, calendar_events.id"#,
            user_ids
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(RunningCheckInItem::from)
        .collect())
    }

    pub async fn list_schedule_runs(
        &self,
        executor: impl PgExecutor<'_>,
        user_ids: &[Uuid],
        notification_window: NotificationWindow,
    ) -> Result<Vec<(Uuid, DueScheduleRun)>, sqlx::Error> {
        Ok(sqlx::query_as!(
            ScheduleRunRow,
            r#"SELECT schedule_runs.user_id,
                      schedule_runs.id,
                      cards.id AS card_id,
                      boards.id AS board_id,
                      cards.title AS card_title,
                      boards.name AS board_name,
                      CASE WHEN schedule_runs.moved THEN board_lists.name END AS moved_to_list_name,
                      (extract(epoch FROM schedule_runs.ran_at) * 1000)::bigint AS "ran_at_millis!"
               FROM schedule_runs
               JOIN card_schedules ON card_schedules.id = schedule_runs.schedule_id
                                  AND card_schedules.user_id = schedule_runs.user_id
               JOIN cards ON cards.id = card_schedules.card_id
               JOIN boards ON boards.id = cards.board_id
               LEFT JOIN board_lists ON board_lists.id = card_schedules.target_list_id
               WHERE schedule_runs.user_id = ANY($1)
                 AND schedule_runs.how IN ('on_time', 'late')
                 AND schedule_runs.ran_at > 'epoch'::timestamptz + $2::bigint * interval '1 millisecond'
                 AND schedule_runs.ran_at <= 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
               ORDER BY schedule_runs.user_id, schedule_runs.ran_at, schedule_runs.id"#,
            user_ids,
            notification_window.after.unix_millis(),
            notification_window.until.unix_millis(),
        )
        .fetch_all(executor)
        .await?
        .into_iter()
        .map(|run_row| {
            (
                run_row.user_id,
                DueScheduleRun {
                    run_id: run_row.id,
                    card_id: run_row.card_id,
                    board_id: run_row.board_id,
                    card_title: run_row.card_title,
                    board_name: run_row.board_name,
                    moved_to_list_name: run_row.moved_to_list_name,
                    ran_at: UtcInstant::from_unix_millis(run_row.ran_at_millis),
                },
            )
        })
        .collect())
    }
}

#[cfg(test)]
pub mod test_support {
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::core::request_time_zone::RequestTimeZone;
    use crate::core::utc_instant::UtcInstant;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;

    pub fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("test instants are valid")
    }

    pub async fn seed_task(db_pool: &PgPool, user_id: Uuid, estimate_minutes: Option<i32>) -> Uuid {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, "DaiBX")
            .await
            .expect("the group should be created");
        let task_id = AddTask::new(db_pool)
            .execute(
                user_id,
                task_group.id,
                "Write the changelog",
                None,
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the task should be added")
            .tasks
            .first()
            .expect("the added task should be listed")
            .id;
        sqlx::query("UPDATE tasks SET estimate_minutes = $2 WHERE id = $1")
            .bind(task_id)
            .bind(estimate_minutes)
            .execute(db_pool)
            .await
            .expect("the estimate should be set");
        task_id
    }

    pub async fn seed_running(
        db_pool: &PgPool,
        user_id: Uuid,
        task_id: Option<Uuid>,
        starts_at: &str,
        last_answered_at: Option<&str>,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO calendar_events (id, user_id, kind, title, task_id, starts_at, last_answered_at)
             VALUES (gen_random_uuid(), $1, 'tracked', 'Write the changelog', $2, $3::timestamptz, $4::timestamptz)
             RETURNING id",
        )
        .bind(user_id)
        .bind(task_id)
        .bind(starts_at)
        .bind(last_answered_at)
        .fetch_one(db_pool)
        .await
        .expect("the running event should be inserted")
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::time;
    use sqlx::PgPool;

    use super::test_support::{instant, seed_running, seed_task};
    use super::*;
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::card_schedules::actions::create_card_schedule::CreateCardSchedule;
    use crate::features::card_schedules::entities::card_schedule::{
        ScheduleKind, ScheduleSettings, ScheduleTiming,
    };
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::notifications::data_sources::postgres::push_device_source::test_support::{
        seed_recipient, seed_user_with_session,
    };

    async fn seed_schedule(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid) {
        let board = CreateBoard::new()
            .execute(db_pool, user_id, "Projects")
            .await
            .expect("the board should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(
                user_id,
                board.board.id,
                "Doing",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the list should be created");
        let doing_list_id = board.lists[0].id;
        let board = CreateCard::new(db_pool)
            .execute(
                user_id,
                doing_list_id,
                "Weekly review",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the card should be created");
        let card_id = board.cards[0].id;
        let schedule_settings = ScheduleSettings {
            target_list_id: doing_list_id,
            timing: ScheduleTiming::from_input(
                ScheduleKind::Repeating,
                "2026-11-09T18:00",
                "Asia/Manila",
                None,
                Some(
                    serde_json::from_str(
                        r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
                    )
                    .expect("a valid rule"),
                ),
            )
            .expect("a valid timing"),
            block: None,
        };
        let schedule_id = CreateCardSchedule::new(db_pool)
            .execute(
                user_id,
                card_id,
                &schedule_settings,
                instant("2026-11-08T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created")[0]
            .id;
        (schedule_id, card_id)
    }

    async fn seed_run(
        db_pool: &PgPool,
        user_id: Uuid,
        schedule_id: Uuid,
        ran_at: &str,
        how: &str,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO schedule_runs (id, user_id, schedule_id, occurrence_at, ran_at, how, moved)
             VALUES (gen_random_uuid(), $1, $2, $3::timestamptz, $3::timestamptz, $4, true)
             RETURNING id",
        )
        .bind(user_id)
        .bind(schedule_id)
        .bind(ran_at)
        .bind(how)
        .fetch_one(db_pool)
        .await
        .expect("the run should be inserted")
    }

    #[sqlx::test]
    async fn lists_only_users_with_a_device_with_their_settings(db_pool: PgPool) {
        let recipient_id = seed_recipient(&db_pool, "phone@example.com").await;
        seed_user_with_session(&db_pool, "no-device@example.com").await;

        let recipients = PostgresDueItemSource
            .list_recipients(&db_pool)
            .await
            .expect("the list should run");

        assert_eq!(
            recipients,
            vec![NotificationRecipient {
                user_id: recipient_id,
                notification_settings: NotificationSettings {
                    check_ins: true,
                    schedule_runs: true,
                    blocks_starting: true,
                    quiet_outside_working_hours: true,
                },
                check_in_settings: CheckInSettings {
                    enabled: true,
                    interval_minutes: 30,
                    first_check_in_minutes: 30,
                },
                working_hours: WallClockWindow::new(time(16, 0, 0, 0), time(2, 0, 0, 0)),
                calendar_time_zone: None,
            }]
        );
    }

    #[sqlx::test]
    async fn lists_running_tracked_events_of_the_given_users_only(db_pool: PgPool) {
        let user_id = seed_recipient(&db_pool, "phone@example.com").await;
        let other_user_id = seed_recipient(&db_pool, "other@example.com").await;
        let task_id = seed_task(&db_pool, user_id, Some(45)).await;
        let running_id = seed_running(
            &db_pool,
            user_id,
            Some(task_id),
            "2026-11-09T09:00:00Z",
            Some("2026-11-09T09:30:00Z"),
        )
        .await;
        sqlx::query(
            "INSERT INTO calendar_events (id, user_id, kind, title, starts_at, ends_at)
             VALUES (gen_random_uuid(), $1, 'tracked', 'Done already', '2026-11-09T08:00:00Z', '2026-11-09T08:30:00Z')",
        )
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("the ended event should be inserted");
        seed_running(&db_pool, other_user_id, None, "2026-11-09T09:00:00Z", None).await;

        let running_check_ins = PostgresDueItemSource
            .list_running_check_ins(&db_pool, &[user_id])
            .await
            .expect("the list should run");

        assert_eq!(
            running_check_ins,
            vec![RunningCheckInItem {
                user_id,
                event_id: running_id,
                item_title: String::from("Write the changelog"),
                context_name: Some(String::from("DaiBX")),
                running_check_in: RunningCheckIn {
                    started_at: instant("2026-11-09T09:00:00Z"),
                    last_answered_at: Some(instant("2026-11-09T09:30:00Z")),
                    estimate_minutes: Some(45),
                },
            }]
        );
    }

    #[sqlx::test]
    async fn lists_on_time_and_late_runs_of_the_last_five_minutes_only(db_pool: PgPool) {
        let user_id = seed_recipient(&db_pool, "phone@example.com").await;
        let other_user_id = seed_recipient(&db_pool, "other@example.com").await;
        let (schedule_id, card_id) = seed_schedule(&db_pool, user_id).await;
        let (other_schedule_id, _) = seed_schedule(&db_pool, other_user_id).await;
        let on_time_run_id = seed_run(
            &db_pool,
            user_id,
            schedule_id,
            "2026-11-09T09:58:00Z",
            "on_time",
        )
        .await;
        let late_run_id = seed_run(
            &db_pool,
            user_id,
            schedule_id,
            "2026-11-09T09:59:00Z",
            "late",
        )
        .await;
        seed_run(
            &db_pool,
            user_id,
            schedule_id,
            "2026-11-09T09:59:30Z",
            "by_hand",
        )
        .await;
        seed_run(
            &db_pool,
            user_id,
            schedule_id,
            "2026-11-09T09:55:00Z",
            "on_time",
        )
        .await;
        seed_run(
            &db_pool,
            other_user_id,
            other_schedule_id,
            "2026-11-09T09:58:00Z",
            "on_time",
        )
        .await;

        let due_schedule_runs = PostgresDueItemSource
            .list_schedule_runs(
                &db_pool,
                &[user_id],
                NotificationWindow::ending_at(instant("2026-11-09T10:00:00Z")),
            )
            .await
            .expect("the list should run");

        let listed_run_ids: Vec<Uuid> = due_schedule_runs
            .iter()
            .map(|(_, due_schedule_run)| due_schedule_run.run_id)
            .collect();
        assert_eq!(listed_run_ids, vec![on_time_run_id, late_run_id]);
        assert_eq!(
            due_schedule_runs[0],
            (
                user_id,
                DueScheduleRun {
                    run_id: on_time_run_id,
                    card_id,
                    board_id: due_schedule_runs[0].1.board_id,
                    card_title: String::from("Weekly review"),
                    board_name: String::from("Projects"),
                    moved_to_list_name: Some(String::from("Doing")),
                    ran_at: instant("2026-11-09T09:58:00Z"),
                },
            )
        );
    }
}
