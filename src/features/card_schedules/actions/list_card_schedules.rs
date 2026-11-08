use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct ListCardSchedules {
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl ListCardSchedules {
    pub fn new() -> Self {
        Self {
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        card_id: Uuid,
    ) -> Result<Vec<CardSchedule>, ScheduleError> {
        Ok(self
            .postgres_card_schedule_source
            .list_for_card(&mut *connection, card_id)
            .await?)
    }
}
