use std::collections::{HashMap, HashSet};

use futures_util::{StreamExt, stream};
use jiff::tz::TimeZone;
use reqwest::Client;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::config::vapid_settings::VapidSettings;
use crate::core::recurrence::to_local;
use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::format_wall_clock_time;
use crate::features::notifications::actions::find_due_items::FindDueItems;
use crate::features::notifications::actions::remove_gone_push_device::RemoveGonePushDevice;
use crate::features::notifications::data_sources::postgres::push_delivery_source::{
    DeliveryClaim, PostgresPushDeliverySource,
};
use crate::features::notifications::data_sources::postgres::push_device_source::PostgresPushDeviceSource;
use crate::features::notifications::data_sources::web_push::web_push_source::{
    PushOutcome, WebPushSource,
};
use crate::features::notifications::entities::due_item::{
    DueItem, NotificationRecipient, RecipientDueItems,
};
use crate::features::notifications::entities::push_device::PushDevice;
use crate::features::notifications::entities::push_device_keys::PushDeviceKeys;
use crate::features::notifications::entities::push_message::{
    BlockStartNotice, PushMessage, ScheduleRunNotice,
};
use crate::features::notifications::entities::quiet_hours::{QuietHours, notification_time_zone};
use crate::features::notifications::error::NotificationError;

pub const PUSH_SEND_BUDGET: usize = 500;
pub const PUSH_SENDS_AT_ONCE: usize = 8;
pub const DELIVERY_KEEP_DAYS: i64 = 2;
const UNNAMED_CHECK_IN_CONTEXT: &str = "Time tracking";
const MILLIS_PER_MINUTE: i64 = 60_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PushTally {
    pub sent_count: usize,
    pub held_back_count: usize,
    pub gone_count: usize,
    pub failed_count: usize,
    pub pruned_count: u64,
}

struct ClaimedSend {
    device_id: Uuid,
    endpoint: String,
    keys: PushDeviceKeys,
    push_message: PushMessage,
}

pub struct SendDueNotifications<'a> {
    db_pool: &'a PgPool,
    find_due_items: FindDueItems<'a>,
    postgres_push_device_source: PostgresPushDeviceSource,
    postgres_push_delivery_source: PostgresPushDeliverySource,
    web_push_source: WebPushSource<'a>,
    remove_gone_push_device: RemoveGonePushDevice<'a>,
}

impl<'a> SendDueNotifications<'a> {
    pub fn new(
        db_pool: &'a PgPool,
        http_client: &'a Client,
        vapid_settings: &'a VapidSettings,
    ) -> Self {
        Self {
            db_pool,
            find_due_items: FindDueItems::new(db_pool),
            postgres_push_device_source: PostgresPushDeviceSource,
            postgres_push_delivery_source: PostgresPushDeliverySource,
            web_push_source: WebPushSource::new(http_client, vapid_settings),
            remove_gone_push_device: RemoveGonePushDevice::new(db_pool),
        }
    }

    pub async fn execute(
        &self,
        now: UtcInstant,
        send_budget: usize,
    ) -> Result<PushTally, NotificationError> {
        let mut push_tally = PushTally::default();
        let recipient_due_items = self.find_due_items.execute(now).await?;
        let claimed_sends = self
            .claim_sends(&recipient_due_items, now, send_budget, &mut push_tally)
            .await?;
        self.send_claimed(claimed_sends, &mut push_tally).await?;
        push_tally.pruned_count = self
            .postgres_push_delivery_source
            .prune_sent_before(self.db_pool, now.plus_days(-DELIVERY_KEEP_DAYS))
            .await?;
        Ok(push_tally)
    }

    async fn claim_sends(
        &self,
        recipient_due_items: &[RecipientDueItems],
        now: UtcInstant,
        send_budget: usize,
        push_tally: &mut PushTally,
    ) -> Result<Vec<ClaimedSend>, NotificationError> {
        let mut claimed_sends = Vec::new();
        if recipient_due_items.is_empty() {
            return Ok(claimed_sends);
        }
        let user_ids: Vec<Uuid> = recipient_due_items
            .iter()
            .map(|recipient_due| recipient_due.recipient.user_id)
            .collect();
        let mut devices_by_user: HashMap<Uuid, Vec<PushDevice>> = HashMap::new();
        for push_device in self
            .postgres_push_device_source
            .list_for_users(self.db_pool, &user_ids)
            .await?
        {
            devices_by_user
                .entry(push_device.user_id)
                .or_default()
                .push(push_device);
        }
        for recipient_due in recipient_due_items {
            let recipient = &recipient_due.recipient;
            for push_device in devices_by_user
                .remove(&recipient.user_id)
                .unwrap_or_default()
            {
                let quiet_hours = quiet_hours_for(recipient, &push_device);
                let shown_time_zone = notification_time_zone(
                    recipient.calendar_time_zone.as_deref(),
                    &push_device.time_zone,
                )
                .unwrap_or(TimeZone::UTC);
                for due_item in &recipient_due.due_items {
                    if claimed_sends.len() >= send_budget {
                        return Ok(claimed_sends);
                    }
                    if quiet_hours.as_ref().is_some_and(|quiet_hours| {
                        quiet_hours.holds_back(due_item.due_at(), due_item.check_in_started_at())
                    }) {
                        push_tally.held_back_count += 1;
                        continue;
                    }
                    let subject_key = due_item.subject_key();
                    let is_claimed = self
                        .postgres_push_delivery_source
                        .claim(
                            self.db_pool,
                            &DeliveryClaim {
                                device_id: push_device.id,
                                user_id: recipient.user_id,
                                kind: due_item.kind().as_str(),
                                subject_key: &subject_key,
                                due_at: due_item.due_at(),
                            },
                        )
                        .await?;
                    if is_claimed {
                        claimed_sends.push(ClaimedSend {
                            device_id: push_device.id,
                            endpoint: push_device.endpoint.clone(),
                            keys: push_device.keys.clone(),
                            push_message: due_message(due_item, &shown_time_zone, now),
                        });
                    }
                }
            }
        }
        Ok(claimed_sends)
    }

    async fn send_claimed(
        &self,
        claimed_sends: Vec<ClaimedSend>,
        push_tally: &mut PushTally,
    ) -> Result<(), NotificationError> {
        let send_results: Vec<_> = stream::iter(claimed_sends)
            .map(|claimed_send| async move {
                let push_outcome = self
                    .web_push_source
                    .send(
                        &claimed_send.endpoint,
                        &claimed_send.keys,
                        &claimed_send.push_message,
                    )
                    .await;
                (claimed_send.device_id, push_outcome)
            })
            .buffer_unordered(PUSH_SENDS_AT_ONCE)
            .collect()
            .await;
        let mut delivered_device_ids = HashSet::new();
        let mut gone_device_ids = HashSet::new();
        for (device_id, push_outcome) in send_results {
            match push_outcome {
                Ok(PushOutcome::Delivered) => {
                    push_tally.sent_count += 1;
                    delivered_device_ids.insert(device_id);
                }
                Ok(PushOutcome::Gone) => {
                    gone_device_ids.insert(device_id);
                }
                Ok(PushOutcome::Refused(refused_status)) => {
                    push_tally.failed_count += 1;
                    tracing::warn!(device_id = %device_id, refused_status, "due notification refused");
                }
                Err(web_push_error) => {
                    push_tally.failed_count += 1;
                    tracing::warn!(device_id = %device_id, error = %web_push_error, "due notification failed");
                }
            }
        }
        for device_id in delivered_device_ids.difference(&gone_device_ids) {
            self.postgres_push_device_source
                .mark_delivered(self.db_pool, *device_id)
                .await?;
        }
        for device_id in &gone_device_ids {
            self.remove_gone_push_device.execute(*device_id).await?;
        }
        push_tally.gone_count = gone_device_ids.len();
        Ok(())
    }
}

fn quiet_hours_for(
    recipient: &NotificationRecipient,
    push_device: &PushDevice,
) -> Option<QuietHours> {
    if !recipient.notification_settings.quiet_outside_working_hours {
        return None;
    }
    QuietHours::new(
        recipient.working_hours?,
        recipient.calendar_time_zone.as_deref(),
        &push_device.time_zone,
    )
}

fn local_clock_text(instant: UtcInstant, time_zone: &TimeZone) -> Option<String> {
    to_local(instant, time_zone).map(|local_moment| format_wall_clock_time(local_moment.time()))
}

fn due_message(due_item: &DueItem, time_zone: &TimeZone, now: UtcInstant) -> PushMessage {
    match due_item {
        DueItem::CheckIn(due_check_in) => PushMessage::check_in(
            due_check_in.event_id,
            &due_check_in.item_title,
            due_check_in
                .context_name
                .as_deref()
                .unwrap_or(UNNAMED_CHECK_IN_CONTEXT),
            (now.unix_millis() - due_check_in.started_at.unix_millis()) / MILLIS_PER_MINUTE,
        ),
        DueItem::ScheduleRun(due_schedule_run) => PushMessage::schedule_ran(&ScheduleRunNotice {
            run_id: due_schedule_run.run_id,
            card_id: due_schedule_run.card_id,
            board_id: due_schedule_run.board_id,
            card_title: &due_schedule_run.card_title,
            board_name: &due_schedule_run.board_name,
            moved_to_list_name: due_schedule_run.moved_to_list_name.as_deref(),
        }),
        DueItem::BlockStart(due_block_start) => {
            let time_span_text = [Some(due_block_start.starts_at), due_block_start.ends_at]
                .into_iter()
                .flatten()
                .filter_map(|instant| local_clock_text(instant, time_zone))
                .collect::<Vec<_>>()
                .join("–");
            PushMessage::block_starting(&BlockStartNotice {
                subject_key: &due_block_start.subject_key,
                event_title: &due_block_start.event_title,
                time_span_text: &time_span_text,
                linked_task_id: due_block_start.linked_task_id,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, header_exists, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::core::http_client::build_http_client;
    use crate::features::notifications::data_sources::postgres::due_item_source::test_support::{
        instant, seed_running, seed_task,
    };
    use crate::features::notifications::data_sources::postgres::push_device_source::test_support::{
        new_device, seed_user_with_session,
    };

    const NOW: &str = "2026-11-09T10:00:00Z";
    const TEST_VAPID_PRIVATE_KEY: &str = "RS0WdYWWo1HajXg3NZR1olzCf31i-ZBGDkFyCs7j1jw";

    struct PushService {
        mock_server: MockServer,
    }

    impl PushService {
        async fn start(gone_path: Option<&str>) -> Self {
            let mock_server = MockServer::start().await;
            if let Some(gone_path) = gone_path {
                Mock::given(method("POST"))
                    .and(path_regex(format!("^/push/{gone_path}$")))
                    .respond_with(ResponseTemplate::new(410))
                    .with_priority(1)
                    .mount(&mock_server)
                    .await;
            }
            Mock::given(method("POST"))
                .and(path_regex("^/push/[a-z-]+$"))
                .and(header("content-encoding", "aes128gcm"))
                .and(header_exists("authorization"))
                .respond_with(ResponseTemplate::new(201))
                .with_priority(2)
                .mount(&mock_server)
                .await;
            Self { mock_server }
        }

        async fn received_paths(&self) -> Vec<String> {
            let mut received_paths: Vec<String> = self
                .mock_server
                .received_requests()
                .await
                .unwrap_or_default()
                .iter()
                .map(|received_request| received_request.url.path().to_owned())
                .collect();
            received_paths.sort();
            received_paths
        }
    }

    async fn seed_device_at(
        db_pool: &PgPool,
        push_service: &PushService,
        user_id: Uuid,
        session_id: Uuid,
        device_name: &str,
    ) -> Uuid {
        let push_device = PostgresPushDeviceSource
            .insert(db_pool, &new_device(user_id, session_id, device_name))
            .await
            .expect("the device should be inserted");
        sqlx::query("UPDATE push_devices SET endpoint = $2 WHERE id = $1")
            .bind(push_device.id)
            .bind(format!(
                "{}/push/{device_name}",
                push_service.mock_server.uri()
            ))
            .execute(db_pool)
            .await
            .expect("the endpoint should point at the fake push service");
        push_device.id
    }

    async fn send_due(db_pool: &PgPool, send_budget: usize) -> PushTally {
        let http_client = build_http_client();
        let vapid_settings = VapidSettings::from_private_key(
            TEST_VAPID_PRIVATE_KEY,
            String::from("https://dai-bx.vercel.app"),
        )
        .expect("test VAPID key should parse");
        SendDueNotifications::new(db_pool, &http_client, &vapid_settings)
            .execute(instant(NOW), send_budget)
            .await
            .expect("sending should run")
    }

    async fn device_ids(db_pool: &PgPool) -> Vec<Uuid> {
        sqlx::query_scalar("SELECT id FROM push_devices ORDER BY id")
            .fetch_all(db_pool)
            .await
            .expect("devices should be readable")
    }

    #[sqlx::test]
    async fn sends_each_due_item_once_to_every_device(db_pool: PgPool) {
        let push_service = PushService::start(None).await;
        let (user_id, session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "phone").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "laptop").await;
        seed_running(&db_pool, user_id, None, "2026-11-09T09:28:00Z", None).await;

        let first_tally = send_due(&db_pool, PUSH_SEND_BUDGET).await;
        let second_tally = send_due(&db_pool, PUSH_SEND_BUDGET).await;

        assert_eq!(first_tally.sent_count, 2);
        assert_eq!(second_tally.sent_count, 0);
        assert_eq!(
            push_service.received_paths().await,
            vec![String::from("/push/laptop"), String::from("/push/phone")]
        );
        let delivered_device_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM push_devices WHERE last_delivered_at IS NOT NULL",
        )
        .fetch_one(&db_pool)
        .await
        .expect("devices should be readable");
        assert_eq!(delivered_device_count, 2);
    }

    #[sqlx::test]
    async fn holds_back_outside_working_hours_unless_the_check_in_started_outside(db_pool: PgPool) {
        let push_service = PushService::start(None).await;
        let (user_id, session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "phone").await;
        sqlx::query(
            "UPDATE users SET working_hours_start = '09:00', working_hours_end = '17:30' WHERE id = $1",
        )
        .bind(user_id)
        .execute(&db_pool)
        .await
        .expect("working hours should change");
        let inside_task_id = seed_task(&db_pool, user_id, Some(30)).await;
        let outside_task_id = seed_task(&db_pool, user_id, Some(5)).await;
        seed_running(
            &db_pool,
            user_id,
            Some(inside_task_id),
            "2026-11-09T09:28:00Z",
            None,
        )
        .await;
        let outside_running_id = seed_running(
            &db_pool,
            user_id,
            Some(outside_task_id),
            "2026-11-09T09:54:00Z",
            None,
        )
        .await;

        let quiet_tally = send_due(&db_pool, PUSH_SEND_BUDGET).await;

        assert_eq!(
            (quiet_tally.sent_count, quiet_tally.held_back_count),
            (1, 1)
        );
        let sent_subjects: Vec<String> =
            sqlx::query_scalar("SELECT subject_key FROM push_deliveries")
                .fetch_all(&db_pool)
                .await
                .expect("deliveries should be readable");
        assert_eq!(sent_subjects, vec![outside_running_id.to_string()]);

        sqlx::query("UPDATE users SET notify_quiet_outside_working_hours = false WHERE id = $1")
            .bind(user_id)
            .execute(&db_pool)
            .await
            .expect("the switch should change");
        let loud_tally = send_due(&db_pool, PUSH_SEND_BUDGET).await;

        assert_eq!((loud_tally.sent_count, loud_tally.held_back_count), (1, 0));
    }

    #[sqlx::test]
    async fn removes_a_gone_device_and_still_reaches_the_others(db_pool: PgPool) {
        let push_service = PushService::start(Some("old-phone")).await;
        let (user_id, session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "old-phone").await;
        let kept_device_id =
            seed_device_at(&db_pool, &push_service, user_id, session_id, "laptop").await;
        seed_running(&db_pool, user_id, None, "2026-11-09T09:28:00Z", None).await;

        let push_tally = send_due(&db_pool, PUSH_SEND_BUDGET).await;

        assert_eq!((push_tally.sent_count, push_tally.gone_count), (1, 1));
        assert_eq!(device_ids(&db_pool).await, vec![kept_device_id]);
    }

    #[sqlx::test]
    async fn stops_claiming_at_the_budget_and_sends_the_rest_next_run(db_pool: PgPool) {
        let push_service = PushService::start(None).await;
        let (user_id, session_id) = seed_user_with_session(&db_pool, "a@example.com").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "phone").await;
        seed_device_at(&db_pool, &push_service, user_id, session_id, "laptop").await;
        seed_running(&db_pool, user_id, None, "2026-11-09T09:28:00Z", None).await;

        let first_tally = send_due(&db_pool, 1).await;
        let second_tally = send_due(&db_pool, 1).await;

        assert_eq!((first_tally.sent_count, second_tally.sent_count), (1, 1));
        assert_eq!(push_service.received_paths().await.len(), 2);
    }

    #[test]
    fn a_block_shows_its_local_start_and_end() {
        let block_message = due_message(
            &DueItem::BlockStart(
                crate::features::notifications::entities::due_item::DueBlockStart {
                    subject_key: String::from("event"),
                    event_title: String::from("Deep work"),
                    starts_at: instant("2026-11-09T08:00:00Z"),
                    ends_at: Some(instant("2026-11-09T10:00:00Z")),
                    linked_task_id: None,
                },
            ),
            &TimeZone::get("Asia/Manila").expect("a known zone"),
            instant(NOW),
        );

        assert_eq!(block_message.body, "16:00–18:00");
    }
}
