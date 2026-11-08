use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::end_schedule_blocks::EndScheduleBlocks;
use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
use crate::features::card_schedules::actions::place_schedule_block::PlaceScheduleBlock;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::{
    CardSchedule, ScheduleSettings, ScheduleWrite,
};
use crate::features::card_schedules::error::ScheduleError;

pub struct UpdateCardSchedule<'a> {
    db_pool: &'a PgPool,
    lock_schedule: LockSchedule,
    end_schedule_blocks: EndScheduleBlocks,
    place_schedule_block: PlaceScheduleBlock,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl<'a> UpdateCardSchedule<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_schedule: LockSchedule::new(),
            end_schedule_blocks: EndScheduleBlocks::new(),
            place_schedule_block: PlaceScheduleBlock::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        schedule_id: Uuid,
        schedule_settings: &ScheduleSettings,
        now: UtcInstant,
    ) -> Result<Vec<CardSchedule>, ScheduleError> {
        let first_run = schedule_settings.first_run_after(now)?;
        let mut update_transaction = self.db_pool.begin().await?;
        let current_schedule = self
            .lock_schedule
            .execute(&mut update_transaction, user_id, schedule_id)
            .await?;
        let schedule_write = self
            .postgres_card_schedule_source
            .update(
                &mut *update_transaction,
                schedule_id,
                schedule_settings,
                first_run,
            )
            .await?;
        if schedule_write == ScheduleWrite::UnknownLink {
            return Err(ScheduleError::NotFound);
        }
        let changed_schedule = self
            .postgres_card_schedule_source
            .find_locked(&mut *update_transaction, user_id, schedule_id)
            .await?
            .ok_or(ScheduleError::NotFound)?;
        self.end_schedule_blocks
            .execute(&mut update_transaction, &[schedule_id], now)
            .await?;
        self.place_schedule_block
            .execute(&mut update_transaction, user_id, &changed_schedule)
            .await?;
        let card_schedules = self
            .postgres_card_schedule_source
            .list_for_card(&mut *update_transaction, current_schedule.card_id)
            .await?;
        update_transaction.commit().await?;
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
    use crate::features::card_schedules::entities::card_schedule::ScheduleBlock;

    #[sqlx::test]
    async fn keeps_past_blocks_and_moves_only_the_future_ones(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let blocked_settings = ScheduleSettings {
            block: Some(ScheduleBlock {
                minutes: 60,
                category_id: None,
            }),
            ..weekly_settings(seeded_card.to_do_list_id)
        };
        let created_schedules = CreateCardSchedule::new(&db_pool)
            .execute(
                user_id,
                seeded_card.card_id,
                &blocked_settings,
                instant("2030-01-01T00:00:00Z"),
            )
            .await
            .expect("the schedule should be created");

        let changed_schedules = UpdateCardSchedule::new(&db_pool)
            .execute(
                user_id,
                created_schedules[0].id,
                &ScheduleSettings {
                    target_list_id: seeded_card.doing_list_id,
                    ..blocked_settings
                },
                instant("2030-01-20T00:00:00Z"),
            )
            .await
            .expect("the schedule should change");

        assert_eq!(
            changed_schedules[0].next_run_at,
            Some(instant("2030-01-21T10:00:00Z"))
        );
        assert_eq!(
            changed_schedules[0].target_list_name.as_deref(),
            Some("Doing")
        );
        let block_series = sqlx::query!(
            r#"SELECT schedule_id,
                      (extract(epoch FROM until_instant) * 1000)::bigint AS until_millis,
                      to_char(local_start, 'YYYY-MM-DD"T"HH24:MI') AS "local_start_text!"
               FROM event_series WHERE user_id = $1 ORDER BY id"#,
            user_id,
        )
        .fetch_all(&db_pool)
        .await
        .expect("the block series");
        let block_series: Vec<(Option<Uuid>, Option<i64>, String)> = block_series
            .into_iter()
            .map(|series_row| {
                (
                    series_row.schedule_id,
                    series_row.until_millis,
                    series_row.local_start_text,
                )
            })
            .collect();
        assert_eq!(
            block_series,
            vec![
                (
                    None,
                    Some(instant("2030-01-20T00:00:00Z").unix_millis()),
                    String::from("2030-01-07T18:00"),
                ),
                (
                    Some(created_schedules[0].id),
                    None,
                    String::from("2030-01-21T18:00"),
                ),
            ]
        );
    }
}
