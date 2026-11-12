use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::end_schedule_blocks::EndScheduleBlocks;
use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::card_schedules::error::ScheduleError;

pub struct DeleteCardSchedule<'a> {
    db_pool: &'a PgPool,
    lock_schedule: LockSchedule,
    end_schedule_blocks: EndScheduleBlocks,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl<'a> DeleteCardSchedule<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_schedule: LockSchedule::new(),
            end_schedule_blocks: EndScheduleBlocks::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        schedule_id: Uuid,
        now: UtcInstant,
    ) -> Result<Vec<CardSchedule>, ScheduleError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        let deleted_schedule = self
            .lock_schedule
            .execute(&mut delete_transaction, user_id, schedule_id)
            .await?;
        self.end_schedule_blocks
            .execute(&mut delete_transaction, &[schedule_id], now)
            .await?;
        self.postgres_card_schedule_source
            .delete(&mut *delete_transaction, schedule_id)
            .await?;
        let card_schedules = self
            .postgres_card_schedule_source
            .list_for_card(&mut *delete_transaction, deleted_schedule.card_id)
            .await?;
        delete_transaction.commit().await?;
        Ok(card_schedules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card_schedules::actions::create_card_schedule::CreateCardSchedule;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        instant, seed_card, seed_user_id, weekly_settings,
    };
    use crate::features::card_schedules::entities::card_schedule::{
        ScheduleBlock, ScheduleSettings, ScheduleTiming,
    };

    #[sqlx::test]
    async fn keeps_a_deleted_schedule_s_past_blocks_as_plain_events(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let create_card_schedule = CreateCardSchedule::new(&db_pool);
        let block = Some(ScheduleBlock {
            minutes: 30,
            category_id: None,
            start_time: None,
        });
        create_card_schedule
            .execute(
                user_id,
                seeded_card.card_id,
                &ScheduleSettings {
                    block,
                    ..weekly_settings(seeded_card.to_do_list_id)
                },
                instant("2030-01-01T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created");
        let card_schedules = create_card_schedule
            .execute(
                user_id,
                seeded_card.card_id,
                &ScheduleSettings {
                    block,
                    timing: ScheduleTiming {
                        repeat_rule: None,
                        ..weekly_settings(seeded_card.to_do_list_id).timing
                    },
                    ..weekly_settings(seeded_card.to_do_list_id)
                },
                instant("2030-01-01T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created");

        for card_schedule in &card_schedules {
            DeleteCardSchedule::new(&db_pool)
                .execute(user_id, card_schedule.id, instant("2030-01-08T00:00:00Z"))
                .await
                .expect("the schedule should be deleted");
        }

        let kept_events = sqlx::query_scalar!(
            "SELECT count(*) FROM calendar_events WHERE user_id = $1 AND schedule_id IS NULL",
            user_id,
        )
        .fetch_one(&db_pool)
        .await
        .expect("the count should run");
        let ended_series = sqlx::query_scalar!(
            r#"SELECT (extract(epoch FROM until_instant) * 1000)::bigint AS "until_millis!"
               FROM event_series WHERE user_id = $1 AND schedule_id IS NULL"#,
            user_id,
        )
        .fetch_all(&db_pool)
        .await
        .expect("the read should run");
        assert_eq!(kept_events, Some(1));
        assert_eq!(
            ended_series,
            vec![instant("2030-01-08T00:00:00Z").unix_millis()]
        );
        let missing_delete = DeleteCardSchedule::new(&db_pool)
            .execute(
                user_id,
                card_schedules[0].id,
                instant("2030-01-08T00:00:00Z"),
            )
            .await;
        assert!(matches!(missing_delete, Err(ScheduleError::NotFound)));
    }
}
