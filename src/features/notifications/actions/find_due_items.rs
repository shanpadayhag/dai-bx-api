use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::actions::list_planned_starts::ListPlannedStarts;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventSource,
};
use crate::features::notifications::data_sources::postgres::due_item_source::PostgresDueItemSource;
use crate::features::notifications::entities::check_in_due::check_in_due_at;
use crate::features::notifications::entities::due_item::{
    DueBlockStart, DueCheckIn, DueItem, NotificationRecipient, NotificationWindow,
    RecipientDueItems,
};
use crate::features::notifications::error::NotificationError;

pub struct FindDueItems<'a> {
    db_pool: &'a PgPool,
    postgres_due_item_source: PostgresDueItemSource,
}

impl<'a> FindDueItems<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_due_item_source: PostgresDueItemSource,
        }
    }

    pub async fn execute(
        &self,
        now: UtcInstant,
    ) -> Result<Vec<RecipientDueItems>, NotificationError> {
        let notification_window = NotificationWindow::ending_at(now);
        let recipients = self
            .postgres_due_item_source
            .list_recipients(self.db_pool)
            .await?;
        let mut due_items_by_user: HashMap<Uuid, Vec<DueItem>> = HashMap::new();
        self.collect_check_ins(&recipients, notification_window, &mut due_items_by_user)
            .await?;
        self.collect_schedule_runs(&recipients, notification_window, &mut due_items_by_user)
            .await?;
        self.collect_block_starts(&recipients, notification_window, &mut due_items_by_user)
            .await?;
        Ok(recipients
            .into_iter()
            .filter_map(|recipient| {
                due_items_by_user
                    .remove(&recipient.user_id)
                    .map(|due_items| RecipientDueItems {
                        recipient,
                        due_items,
                    })
            })
            .collect())
    }

    async fn collect_check_ins(
        &self,
        recipients: &[NotificationRecipient],
        notification_window: NotificationWindow,
        due_items_by_user: &mut HashMap<Uuid, Vec<DueItem>>,
    ) -> Result<(), NotificationError> {
        let check_in_settings_by_user: HashMap<Uuid, _> = recipients
            .iter()
            .filter(|recipient| recipient.notification_settings.check_ins)
            .map(|recipient| (recipient.user_id, recipient.check_in_settings))
            .collect();
        if check_in_settings_by_user.is_empty() {
            return Ok(());
        }
        let user_ids: Vec<Uuid> = check_in_settings_by_user.keys().copied().collect();
        let running_check_ins = self
            .postgres_due_item_source
            .list_running_check_ins(self.db_pool, &user_ids)
            .await?;
        for running_item in running_check_ins {
            let Some(due_at) = check_in_settings_by_user
                .get(&running_item.user_id)
                .and_then(|check_in_settings| {
                    check_in_due_at(&running_item.running_check_in, check_in_settings)
                })
                .filter(|due_at| notification_window.contains(*due_at))
            else {
                continue;
            };
            due_items_by_user
                .entry(running_item.user_id)
                .or_default()
                .push(DueItem::CheckIn(DueCheckIn {
                    event_id: running_item.event_id,
                    item_title: running_item.item_title,
                    context_name: running_item.context_name,
                    started_at: running_item.running_check_in.started_at,
                    due_at,
                }));
        }
        Ok(())
    }

    async fn collect_schedule_runs(
        &self,
        recipients: &[NotificationRecipient],
        notification_window: NotificationWindow,
        due_items_by_user: &mut HashMap<Uuid, Vec<DueItem>>,
    ) -> Result<(), NotificationError> {
        let user_ids: Vec<Uuid> = recipients
            .iter()
            .filter(|recipient| recipient.notification_settings.schedule_runs)
            .map(|recipient| recipient.user_id)
            .collect();
        if user_ids.is_empty() {
            return Ok(());
        }
        for (user_id, due_schedule_run) in self
            .postgres_due_item_source
            .list_schedule_runs(self.db_pool, &user_ids, notification_window)
            .await?
        {
            due_items_by_user
                .entry(user_id)
                .or_default()
                .push(DueItem::ScheduleRun(due_schedule_run));
        }
        Ok(())
    }

    async fn collect_block_starts(
        &self,
        recipients: &[NotificationRecipient],
        notification_window: NotificationWindow,
        due_items_by_user: &mut HashMap<Uuid, Vec<DueItem>>,
    ) -> Result<(), NotificationError> {
        let list_planned_starts = ListPlannedStarts::new(self.db_pool);
        for recipient in recipients
            .iter()
            .filter(|recipient| recipient.notification_settings.blocks_starting)
        {
            let block_starts: Vec<DueItem> = list_planned_starts
                .execute(recipient.user_id, notification_window.as_event_range())
                .await?
                .into_iter()
                .filter_map(due_block_start)
                .map(DueItem::BlockStart)
                .collect();
            if !block_starts.is_empty() {
                due_items_by_user
                    .entry(recipient.user_id)
                    .or_default()
                    .extend(block_starts);
            }
        }
        Ok(())
    }
}

fn due_block_start(planned_event: CalendarEvent) -> Option<DueBlockStart> {
    let is_finished_task = planned_event
        .linked_task
        .as_ref()
        .is_some_and(|linked_task| linked_task.completed_at.is_some());
    if planned_event.is_schedule_block || is_finished_task {
        return None;
    }
    let subject_key = match &planned_event.series {
        Some(series_occurrence) => format!(
            "{}@{}",
            series_occurrence.series_id,
            series_occurrence.original_start.to_iso_text()
        ),
        None => planned_event.id.to_string(),
    };
    let linked_task_id = match planned_event.source {
        Some(CalendarEventSource::Task { task_id, .. }) => Some(task_id),
        Some(CalendarEventSource::Card { .. }) | None => None,
    };
    Some(DueBlockStart {
        subject_key,
        event_title: planned_event.title,
        starts_at: planned_event.starts_at,
        ends_at: planned_event.ends_at,
        linked_task_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::recurrence::RepeatRule;
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::calendar_events::actions::create_planned_event::CreatePlannedEvent;
    use crate::features::calendar_events::entities::calendar_event::{
        EventRepeat, PlannedEventInput, parse_event_zone,
    };
    use crate::features::card_schedules::actions::create_card_schedule::CreateCardSchedule;
    use crate::features::card_schedules::entities::card_schedule::{
        ScheduleBlock, ScheduleKind, ScheduleSettings, ScheduleTiming,
    };
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::notifications::data_sources::postgres::due_item_source::test_support::{
        instant, seed_running, seed_task,
    };
    use crate::features::notifications::data_sources::postgres::push_device_source::test_support::seed_recipient;

    const NOW: &str = "2026-11-09T10:00:00Z";

    fn weekly_on_monday() -> RepeatRule {
        serde_json::from_str(
            r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
        )
        .expect("a valid rule")
    }

    fn planned_input(title: &str, starts_at: &str) -> PlannedEventInput {
        let starts_at = instant(starts_at);
        PlannedEventInput {
            title: title.to_owned(),
            category_id: None,
            starts_at,
            ends_at: starts_at.plus_minutes(60),
            time_zone: None,
            repeat: None,
            task_id: None,
        }
    }

    async fn find_due_items(db_pool: &PgPool) -> Vec<RecipientDueItems> {
        FindDueItems::new(db_pool)
            .execute(instant(NOW))
            .await
            .expect("finding should run")
    }

    fn due_check_in_ids(recipient_due_items: &RecipientDueItems) -> Vec<(Uuid, UtcInstant)> {
        recipient_due_items
            .due_items
            .iter()
            .filter_map(|due_item| match due_item {
                DueItem::CheckIn(due_check_in) => {
                    Some((due_check_in.event_id, due_check_in.due_at))
                }
                DueItem::ScheduleRun(_) | DueItem::BlockStart(_) => None,
            })
            .collect()
    }

    async fn seed_schedule_with_block(db_pool: &PgPool, user_id: Uuid) -> Uuid {
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
        CreateCardSchedule::new(db_pool)
            .execute(
                user_id,
                board.cards[0].id,
                &ScheduleSettings {
                    target_list_id: doing_list_id,
                    timing: ScheduleTiming::from_input(
                        ScheduleKind::Repeating,
                        "2026-11-09T18:00",
                        "Asia/Manila",
                        None,
                        Some(weekly_on_monday()),
                    )
                    .expect("a valid timing"),
                    block: Some(ScheduleBlock {
                        minutes: 30,
                        category_id: None,
                        start_time: None,
                    }),
                },
                instant("2026-11-08T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created")[0]
            .id
    }

    #[sqlx::test]
    async fn finds_check_ins_that_came_due_in_the_last_five_minutes(db_pool: PgPool) {
        let user_id = seed_recipient(&db_pool, "phone@example.com").await;
        let other_user_id = seed_recipient(&db_pool, "other@example.com").await;
        let estimated_task_id = seed_task(&db_pool, user_id, Some(45)).await;
        let estimate_running_id = seed_running(
            &db_pool,
            user_id,
            Some(estimated_task_id),
            "2026-11-09T09:15:00Z",
            None,
        )
        .await;
        let first_check_in_running_id =
            seed_running(&db_pool, user_id, None, "2026-11-09T09:28:00Z", None).await;
        let answered_running_id = seed_running(
            &db_pool,
            user_id,
            None,
            "2026-11-09T08:00:00Z",
            Some("2026-11-09T09:30:00Z"),
        )
        .await;
        seed_running(&db_pool, user_id, None, "2026-11-09T09:25:00Z", None).await;
        seed_running(&db_pool, user_id, None, "2026-11-09T09:31:00Z", None).await;
        seed_running(&db_pool, other_user_id, None, "2026-11-09T09:20:00Z", None).await;

        let recipient_due_items = find_due_items(&db_pool).await;

        assert_eq!(recipient_due_items.len(), 1);
        assert_eq!(recipient_due_items[0].recipient.user_id, user_id);
        assert_eq!(
            due_check_in_ids(&recipient_due_items[0]),
            vec![
                (answered_running_id, instant("2026-11-09T10:00:00Z")),
                (estimate_running_id, instant("2026-11-09T10:00:00Z")),
                (first_check_in_running_id, instant("2026-11-09T09:58:00Z")),
            ]
        );
    }

    #[sqlx::test]
    async fn leaves_out_check_ins_when_either_switch_is_off(db_pool: PgPool) {
        let tracking_off_id = seed_recipient(&db_pool, "tracking-off@example.com").await;
        let notify_off_id = seed_recipient(&db_pool, "notify-off@example.com").await;
        sqlx::query("UPDATE users SET check_ins_enabled = false WHERE id = $1")
            .bind(tracking_off_id)
            .execute(&db_pool)
            .await
            .expect("the switch should change");
        sqlx::query("UPDATE users SET notify_check_ins = false WHERE id = $1")
            .bind(notify_off_id)
            .execute(&db_pool)
            .await
            .expect("the switch should change");
        for user_id in [tracking_off_id, notify_off_id] {
            seed_running(&db_pool, user_id, None, "2026-11-09T09:28:00Z", None).await;
        }

        assert_eq!(find_due_items(&db_pool).await, Vec::new());
    }

    #[sqlx::test]
    async fn finds_planned_starts_but_not_schedule_blocks_or_older_starts(db_pool: PgPool) {
        let user_id = seed_recipient(&db_pool, "phone@example.com").await;
        let task_id = seed_task(&db_pool, user_id, None).await;
        let create_planned_event = CreatePlannedEvent::new(&db_pool);
        let linked_event = create_planned_event
            .execute(
                user_id,
                &PlannedEventInput {
                    task_id: Some(task_id),
                    ..planned_input("Write the changelog", "2026-11-09T09:57:00Z")
                },
            )
            .await
            .expect("the event should be created");
        let series_event = create_planned_event
            .execute(
                user_id,
                &PlannedEventInput {
                    repeat: Some(EventRepeat {
                        repeat_rule: weekly_on_monday(),
                        input_zone: parse_event_zone("Asia/Manila", "inputZone")
                            .expect("a known zone"),
                    }),
                    ..planned_input("Gym", "2026-11-02T10:00:00Z")
                },
            )
            .await
            .expect("the series should be created");
        create_planned_event
            .execute(user_id, &planned_input("Too early", "2026-11-09T09:55:00Z"))
            .await
            .expect("the event should be created");
        seed_schedule_with_block(&db_pool, user_id).await;

        let recipient_due_items = find_due_items(&db_pool).await;

        assert_eq!(recipient_due_items.len(), 1);
        assert_eq!(
            recipient_due_items[0].due_items,
            vec![
                DueItem::BlockStart(DueBlockStart {
                    subject_key: linked_event.id.to_string(),
                    event_title: String::from("Write the changelog"),
                    starts_at: instant("2026-11-09T09:57:00Z"),
                    ends_at: Some(instant("2026-11-09T10:57:00Z")),
                    linked_task_id: Some(task_id),
                }),
                DueItem::BlockStart(DueBlockStart {
                    subject_key: format!("{}@2026-11-09T10:00:00.000Z", series_event.id),
                    event_title: String::from("Gym"),
                    starts_at: instant("2026-11-09T10:00:00Z"),
                    ends_at: Some(instant("2026-11-09T11:00:00Z")),
                    linked_task_id: None,
                }),
            ]
        );
    }

    #[sqlx::test]
    async fn leaves_out_schedule_runs_and_block_starts_when_switched_off(db_pool: PgPool) {
        let user_id = seed_recipient(&db_pool, "phone@example.com").await;
        let schedule_id = seed_schedule_with_block(&db_pool, user_id).await;
        let run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO schedule_runs (id, user_id, schedule_id, occurrence_at, ran_at, how, moved)
             VALUES (gen_random_uuid(), $1, $2, $3::timestamptz, $3::timestamptz, 'on_time', true)
             RETURNING id",
        )
        .bind(user_id)
        .bind(schedule_id)
        .bind(NOW)
        .fetch_one(&db_pool)
        .await
        .expect("the run should be inserted");
        let planned_event = CreatePlannedEvent::new(&db_pool)
            .execute(user_id, &planned_input("Gym", "2026-11-09T09:58:00Z"))
            .await
            .expect("the event should be created");

        let switched_on_subjects: Vec<String> = find_due_items(&db_pool)
            .await
            .iter()
            .flat_map(|recipient_due_items| recipient_due_items.due_items.iter())
            .map(DueItem::subject_key)
            .collect();
        sqlx::query(
            "UPDATE users SET notify_schedule_runs = false, notify_blocks_starting = false WHERE id = $1",
        )
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("the switches should change");

        assert_eq!(
            switched_on_subjects,
            vec![run_id.to_string(), planned_event.id.to_string()]
        );
        assert_eq!(find_due_items(&db_pool).await, Vec::new());
    }
}
