use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
use crate::features::card_schedules::actions::place_schedule_block::PlaceScheduleBlock;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::data_sources::postgres::schedule_block_source::PostgresScheduleBlockSource;
use crate::features::card_schedules::entities::card_schedule::{CardSchedule, ScheduleKind};
use crate::features::card_schedules::error::ScheduleError;

pub struct SetSchedulePaused<'a> {
    db_pool: &'a PgPool,
    lock_schedule: LockSchedule,
    place_schedule_block: PlaceScheduleBlock,
    postgres_card_schedule_source: PostgresCardScheduleSource,
    postgres_schedule_block_source: PostgresScheduleBlockSource,
}

impl<'a> SetSchedulePaused<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_schedule: LockSchedule::new(),
            place_schedule_block: PlaceScheduleBlock::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
            postgres_schedule_block_source: PostgresScheduleBlockSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        schedule_id: Uuid,
        is_paused: bool,
        now: UtcInstant,
    ) -> Result<CardSchedule, ScheduleError> {
        let mut pause_transaction = self.db_pool.begin().await?;
        let current_schedule = self
            .lock_schedule
            .execute(&mut pause_transaction, user_id, schedule_id)
            .await?;
        if current_schedule.is_paused == is_paused {
            return Ok(current_schedule);
        }
        let next_run_at = if is_paused {
            current_schedule.next_run_at
        } else {
            current_schedule.timing.next_run_after(now)
        };
        self.postgres_card_schedule_source
            .set_paused(&mut *pause_transaction, schedule_id, is_paused, next_run_at)
            .await?;
        self.postgres_schedule_block_source
            .set_series_hidden(&mut *pause_transaction, &[schedule_id], is_paused)
            .await?;
        let changed_schedule = self
            .postgres_card_schedule_source
            .find_locked(&mut *pause_transaction, user_id, schedule_id)
            .await?
            .ok_or(ScheduleError::NotFound)?;
        if is_paused {
            self.postgres_schedule_block_source
                .delete_future_events(&mut *pause_transaction, &[schedule_id], now)
                .await?;
        } else if changed_schedule.timing.kind() == ScheduleKind::Once {
            self.place_schedule_block
                .execute(&mut pause_transaction, user_id, &changed_schedule)
                .await?;
        }
        pause_transaction.commit().await?;
        Ok(changed_schedule)
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

    async fn block_counts(db_pool: &PgPool, user_id: Uuid) -> (Option<i64>, Option<i64>) {
        let shown_series = sqlx::query_scalar!(
            "SELECT count(*) FROM event_series WHERE user_id = $1 AND NOT hidden",
            user_id,
        )
        .fetch_one(db_pool)
        .await
        .expect("the count should run");
        let block_events = sqlx::query_scalar!(
            "SELECT count(*) FROM calendar_events WHERE user_id = $1",
            user_id,
        )
        .fetch_one(db_pool)
        .await
        .expect("the count should run");
        (shown_series, block_events)
    }

    #[sqlx::test]
    async fn hides_blocks_while_paused_and_resumes_from_the_next_future_run(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let block = Some(ScheduleBlock {
            minutes: 30,
            category_id: None,
        });
        let create_card_schedule = CreateCardSchedule::new(&db_pool);
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
        let set_schedule_paused = SetSchedulePaused::new(&db_pool);

        for card_schedule in &card_schedules {
            let paused_schedule = set_schedule_paused
                .execute(
                    user_id,
                    card_schedule.id,
                    true,
                    instant("2030-01-02T00:00:00Z"),
                )
                .await
                .expect("the pause should run");
            assert!(paused_schedule.is_paused);
        }
        let paused_counts = block_counts(&db_pool, user_id).await;
        let resumed_schedules = [
            set_schedule_paused
                .execute(
                    user_id,
                    card_schedules[0].id,
                    false,
                    instant("2030-01-15T00:00:00Z"),
                )
                .await
                .expect("the resume should run"),
            set_schedule_paused
                .execute(
                    user_id,
                    card_schedules[1].id,
                    false,
                    instant("2030-01-03T00:00:00Z"),
                )
                .await
                .expect("the resume should run"),
        ];
        let resumed_counts = block_counts(&db_pool, user_id).await;

        assert_eq!(paused_counts, (Some(0), Some(0)));
        assert_eq!(
            resumed_schedules[0].next_run_at,
            Some(instant("2030-01-21T10:00:00Z"))
        );
        assert_eq!(
            resumed_schedules[1].next_run_at,
            Some(instant("2030-01-07T10:00:00Z"))
        );
        assert_eq!(resumed_counts, (Some(1), Some(1)));
    }
}
