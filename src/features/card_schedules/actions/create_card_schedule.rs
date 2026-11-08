use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::place_schedule_block::PlaceScheduleBlock;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::{
    CardSchedule, NewCardSchedule, SCHEDULES_PER_CARD_MAX, ScheduleSettings, ScheduleWrite,
};
use crate::features::card_schedules::error::ScheduleError;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;

pub struct CreateCardSchedule<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    place_schedule_block: PlaceScheduleBlock,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl<'a> CreateCardSchedule<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            place_schedule_block: PlaceScheduleBlock::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        schedule_settings: &ScheduleSettings,
        now: UtcInstant,
    ) -> Result<Vec<CardSchedule>, ScheduleError> {
        let first_run = schedule_settings.first_run_after(now)?;
        let mut create_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut create_transaction, user_id, card_id)
            .await?;
        let schedule_count = self
            .postgres_card_schedule_source
            .count_for_card(&mut *create_transaction, card_id)
            .await?;
        if schedule_count >= SCHEDULES_PER_CARD_MAX {
            return Err(ScheduleError::Invalid(String::from(
                "a card has at most 20 schedules",
            )));
        }
        let new_schedule = NewCardSchedule {
            id: Uuid::now_v7(),
            user_id,
            card_id,
            board_id: board_summary.id,
            settings: schedule_settings.clone(),
            next_run_at: first_run,
        };
        let schedule_write = self
            .postgres_card_schedule_source
            .insert(&mut *create_transaction, &new_schedule)
            .await?;
        if schedule_write == ScheduleWrite::UnknownLink {
            return Err(ScheduleError::NotFound);
        }
        let created_schedule = self
            .postgres_card_schedule_source
            .find_locked(&mut *create_transaction, user_id, new_schedule.id)
            .await?
            .ok_or(ScheduleError::NotFound)?;
        self.place_schedule_block
            .execute(&mut create_transaction, user_id, &created_schedule)
            .await?;
        let card_schedules = self
            .postgres_card_schedule_source
            .list_for_card(&mut *create_transaction, card_id)
            .await?;
        create_transaction.commit().await?;
        Ok(card_schedules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        instant, seed_card, seed_category_id, seed_user_id, weekly_settings,
    };
    use crate::features::card_schedules::entities::card_schedule::{ScheduleBlock, ScheduleTiming};

    #[sqlx::test]
    async fn creates_a_schedule_with_its_block_series(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let category_id = seed_category_id(&db_pool, user_id).await;

        let card_schedules = CreateCardSchedule::new(&db_pool)
            .execute(
                user_id,
                seeded_card.card_id,
                &ScheduleSettings {
                    block: Some(ScheduleBlock {
                        minutes: 60,
                        category_id: Some(category_id),
                    }),
                    ..weekly_settings(seeded_card.to_do_list_id)
                },
                instant("2030-01-08T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created");

        assert_eq!(card_schedules.len(), 1);
        assert_eq!(
            card_schedules[0].next_run_at,
            Some(instant("2030-01-14T10:00:00Z"))
        );
        let block_series = sqlx::query!(
            r#"SELECT title, category_id, duration_minutes, schedule_id,
                      to_char(local_start, 'YYYY-MM-DD"T"HH24:MI') AS "local_start_text!"
               FROM event_series WHERE user_id = $1"#,
            user_id,
        )
        .fetch_one(&db_pool)
        .await
        .expect("a block series");
        assert_eq!(block_series.title, "Weekly review");
        assert_eq!(block_series.category_id, Some(category_id));
        assert_eq!(block_series.duration_minutes, 60);
        assert_eq!(block_series.schedule_id, Some(card_schedules[0].id));
        assert_eq!(block_series.local_start_text, "2030-01-14T18:00");
    }

    #[sqlx::test]
    async fn refuses_a_21st_schedule_and_a_schedule_with_no_run_left(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let create_card_schedule = CreateCardSchedule::new(&db_pool);
        for _ in 0..SCHEDULES_PER_CARD_MAX {
            create_card_schedule
                .execute(
                    user_id,
                    seeded_card.card_id,
                    &weekly_settings(seeded_card.to_do_list_id),
                    instant("2026-11-08T00:00:00Z"),
                )
                .await
                .expect("the schedule should be created");
        }

        let one_too_many = create_card_schedule
            .execute(
                user_id,
                seeded_card.card_id,
                &weekly_settings(seeded_card.to_do_list_id),
                instant("2026-11-08T00:00:00Z"),
            )
            .await;
        let once_in_the_past = create_card_schedule
            .execute(
                user_id,
                seeded_card.card_id,
                &ScheduleSettings {
                    timing: ScheduleTiming {
                        repeat_rule: None,
                        ..weekly_settings(seeded_card.to_do_list_id).timing
                    },
                    ..weekly_settings(seeded_card.to_do_list_id)
                },
                instant("2031-01-01T00:00:00Z"),
            )
            .await;

        assert!(matches!(one_too_many, Err(ScheduleError::Invalid(_))));
        assert!(matches!(once_in_the_past, Err(ScheduleError::Invalid(_))));
    }
}
