use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::card_schedules::data_sources::postgres::schedule_block_source::PostgresScheduleBlockSource;
use crate::features::card_schedules::entities::card_schedule::{
    CardSchedule, NewScheduleBlock, ScheduleWrite,
};
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct PlaceScheduleBlock {
    postgres_schedule_block_source: PostgresScheduleBlockSource,
}

impl PlaceScheduleBlock {
    pub fn new() -> Self {
        Self {
            postgres_schedule_block_source: PostgresScheduleBlockSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_schedule: &CardSchedule,
    ) -> Result<(), ScheduleError> {
        let (Some(first_run), Some(schedule_settings)) =
            (card_schedule.next_run_at, card_schedule.settings())
        else {
            return Ok(());
        };
        let Some(new_block) = schedule_settings.new_block(&card_schedule.card_title, first_run)
        else {
            return Ok(());
        };
        let is_series = matches!(new_block, NewScheduleBlock::Series { .. });
        if card_schedule.is_paused && !is_series {
            return Ok(());
        }
        let block_write = self
            .postgres_schedule_block_source
            .insert(&mut *connection, user_id, card_schedule.id, &new_block)
            .await?;
        if block_write == ScheduleWrite::UnknownLink {
            return Err(ScheduleError::NotFound);
        }
        if card_schedule.is_paused {
            self.postgres_schedule_block_source
                .set_series_hidden(&mut *connection, &[card_schedule.id], true)
                .await?;
        }
        Ok(())
    }
}
