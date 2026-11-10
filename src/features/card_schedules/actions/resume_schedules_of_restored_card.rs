use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::place_schedule_block::PlaceScheduleBlock;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::data_sources::postgres::schedule_block_source::PostgresScheduleBlockSource;
use crate::features::card_schedules::entities::card_schedule::ScheduleKind;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct ResumeSchedulesOfRestoredCard {
    place_schedule_block: PlaceScheduleBlock,
    postgres_card_schedule_source: PostgresCardScheduleSource,
    postgres_schedule_block_source: PostgresScheduleBlockSource,
}

impl ResumeSchedulesOfRestoredCard {
    pub fn new() -> Self {
        Self {
            place_schedule_block: PlaceScheduleBlock::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
            postgres_schedule_block_source: PostgresScheduleBlockSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_id: Uuid,
        now: UtcInstant,
    ) -> Result<(), ScheduleError> {
        let resumed_schedule_ids = self
            .postgres_card_schedule_source
            .list_ids_paused_by_archive(&mut *connection, card_id)
            .await?;
        if resumed_schedule_ids.is_empty() {
            return Ok(());
        }
        for schedule_id in &resumed_schedule_ids {
            let paused_schedule = self
                .postgres_card_schedule_source
                .find_locked(&mut *connection, user_id, *schedule_id)
                .await?
                .ok_or(ScheduleError::NotFound)?;
            let next_run_at = paused_schedule.timing.next_run_after(now);
            self.postgres_card_schedule_source
                .set_paused(&mut *connection, *schedule_id, false, next_run_at)
                .await?;
            if paused_schedule.timing.kind() == ScheduleKind::Once {
                let resumed_schedule = self
                    .postgres_card_schedule_source
                    .find_locked(&mut *connection, user_id, *schedule_id)
                    .await?
                    .ok_or(ScheduleError::NotFound)?;
                self.place_schedule_block
                    .execute(&mut *connection, user_id, &resumed_schedule)
                    .await?;
            }
        }
        self.postgres_schedule_block_source
            .set_series_hidden(&mut *connection, &resumed_schedule_ids, false)
            .await?;
        Ok(())
    }
}
